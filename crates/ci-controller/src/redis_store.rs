use deadpool_redis::{Config, Connection, Pool, Runtime};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::{info, warn};

/// Worker presence data stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPresence {
    pub status: String,
    pub load: f64,
    pub free_mem_mb: u64,
    pub free_disk_mb: u64,
    pub running_jobs: Vec<String>,
}

/// Job group state cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobGroupCache {
    pub state: String,
    pub stages: std::collections::HashMap<String, String>,
    pub worker_id: Option<String>,
}

/// Redis store with connection pooling via deadpool-redis
pub struct RedisStore {
    pool: Pool,
    prefix: String,
}

impl RedisStore {
    pub async fn new(redis_url: &str, prefix: &str) -> anyhow::Result<Self> {
        let cfg = Config::from_url(redis_url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1))?;

        // Test the connection
        let mut conn = pool.get().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        info!("Connected to Redis (pool)");

        Ok(Self {
            pool,
            prefix: prefix.to_string(),
        })
    }

    async fn get_conn(&self) -> anyhow::Result<Connection> {
        Ok(self.pool.get().await?)
    }

    fn key(&self, parts: &[&str]) -> String {
        format!("{}{}", self.prefix, parts.join(":"))
    }

    // ── Worker Reservation Lock ──

    /// Attempt to reserve a worker for a job group. Returns true if lock acquired.
    pub async fn reserve_worker(
        &self,
        worker_id: &str,
        job_group_id: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<bool> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "reservation", worker_id]);

        // SET NX EX — atomic "set if not exists" with TTL
        let result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(job_group_id)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await?;

        let acquired = result.is_some();
        if acquired {
            info!("Worker {} reserved for group {}", worker_id, job_group_id);
        }
        Ok(acquired)
    }

    /// Release a worker reservation only if owned by `expected_group_id`.
    /// Uses a Lua script for atomic check-and-delete to prevent clobbering
    /// a newer reservation acquired after TTL expiry.
    /// Returns true if the reservation was actually deleted.
    pub async fn release_worker_if_owner(
        &self,
        worker_id: &str,
        expected_group_id: &str,
    ) -> anyhow::Result<bool> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "reservation", worker_id]);

        let script = redis::Script::new(
            r#"
            if redis.call('GET', KEYS[1]) == ARGV[1] then
                redis.call('DEL', KEYS[1])
                return 1
            else
                return 0
            end
            "#,
        );

        let result: i32 = script
            .key(&key)
            .arg(expected_group_id)
            .invoke_async(&mut conn)
            .await?;

        let released = result == 1;
        if released {
            info!(
                "Released worker {} reservation (group {})",
                worker_id, expected_group_id
            );
        } else {
            warn!(
                "Worker {} reservation not owned by group {}, skipping release",
                worker_id, expected_group_id
            );
        }
        Ok(released)
    }

    /// Refresh the TTL on an existing worker reservation without changing its value.
    /// Call on every stage submission to prevent expiry during long pipelines.
    pub async fn refresh_reservation_ttl(
        &self,
        worker_id: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let key = self.key(&["worker", "reservation", worker_id]);
        let mut conn = self.pool.get().await?;
        let _: () = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(ttl_secs as i64)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    /// Unconditionally delete a worker reservation.
    /// Use only when the worker is dead and we must reclaim regardless of owner
    /// (e.g., heartbeat timeout cleanup).
    pub async fn release_worker_force(&self, worker_id: &str) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "reservation", worker_id]);
        let _: () = conn.del(&key).await?;
        info!("Worker {} reservation force-released", worker_id);
        Ok(())
    }

    /// Check if a worker is reserved. Returns the job_group_id if reserved.
    pub async fn get_worker_reservation(&self, worker_id: &str) -> anyhow::Result<Option<String>> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "reservation", worker_id]);
        let result: Option<String> = conn.get(&key).await?;
        Ok(result)
    }

    // ── Worker Presence ──

    /// Update worker presence (called on every heartbeat).
    pub async fn update_worker_presence(
        &self,
        worker_id: &str,
        presence: &WorkerPresence,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "alive", worker_id]);
        let json = serde_json::to_string(presence)?;
        let _: () = conn.set_ex(&key, &json, ttl_secs).await?;
        Ok(())
    }

    /// Get worker presence data.
    pub async fn get_worker_presence(
        &self,
        worker_id: &str,
    ) -> anyhow::Result<Option<WorkerPresence>> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "alive", worker_id]);
        let result: Option<String> = conn.get(&key).await?;
        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Remove worker presence (on disconnect).
    pub async fn remove_worker_presence(&self, worker_id: &str) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "alive", worker_id]);
        let _: () = conn.del(&key).await?;
        Ok(())
    }

    // ── Available Workers Set ──

    /// Add a worker to the available set.
    pub async fn add_available_worker(&self, worker_id: &str) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["workers", "available"]);
        let _: () = conn.sadd(&key, worker_id).await?;
        Ok(())
    }

    /// Remove a worker from the available set.
    pub async fn remove_available_worker(&self, worker_id: &str) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["workers", "available"]);
        let _: () = conn.srem(&key, worker_id).await?;
        Ok(())
    }

    /// Get all available worker IDs.
    pub async fn get_available_workers(&self) -> anyhow::Result<Vec<String>> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["workers", "available"]);
        let result: Vec<String> = conn.smembers(&key).await?;
        Ok(result)
    }

    // ── Job Group State Cache ──

    /// Update the job group state cache.
    pub async fn update_job_group_cache(
        &self,
        group_id: &str,
        cache: &JobGroupCache,
    ) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["jobgroup", group_id, "state"]);
        let json = serde_json::to_string(cache)?;
        let _: () = conn.set_ex(&key, &json, 86400).await?; // 24h TTL
        Ok(())
    }

    /// Get the job group state cache.
    pub async fn get_job_group_cache(
        &self,
        group_id: &str,
    ) -> anyhow::Result<Option<JobGroupCache>> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["jobgroup", group_id, "state"]);
        let result: Option<String> = conn.get(&key).await?;
        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    // ── Pub/Sub ──

    /// Publish an event for a job group.
    pub async fn publish_event(&self, group_id: &str, event: &str) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let channel = self.key(&["events", group_id]);
        let _: () = conn.publish(&channel, event).await?;
        Ok(())
    }

    // ── Distributed Lock (generic) ──

    /// Acquire a generic distributed lock.
    /// Stores a unique owner token so only the acquirer can release it.
    /// Returns `Some(owner_token)` if acquired, `None` otherwise.
    pub async fn acquire_lock(
        &self,
        lock_name: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<Option<String>> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["lock", lock_name]);
        let owner = uuid::Uuid::new_v4().to_string();
        let result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(&owner)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await?;
        Ok(if result.is_some() { Some(owner) } else { None })
    }

    /// Release a generic distributed lock only if we own it.
    /// Uses a Lua script for atomic check-and-delete.
    /// Returns true if the lock was actually released.
    pub async fn release_lock(&self, lock_name: &str, owner: &str) -> anyhow::Result<bool> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["lock", lock_name]);

        let script = redis::Script::new(
            r#"
            if redis.call('GET', KEYS[1]) == ARGV[1] then
                redis.call('DEL', KEYS[1])
                return 1
            else
                return 0
            end
            "#,
        );

        let result: i32 = script.key(&key).arg(owner).invoke_async(&mut conn).await?;

        let released = result == 1;
        if !released {
            warn!(
                "Lock {} not owned by token {}, skipping release",
                lock_name, owner
            );
        }
        Ok(released)
    }

    /// Unconditionally delete a generic distributed lock.
    /// Use only for administrative cleanup.
    pub async fn release_lock_force(&self, lock_name: &str) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["lock", lock_name]);
        let _: () = conn.del(&key).await?;
        Ok(())
    }

    /// Get the key prefix used by this store.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

// ── Keyspace notification helpers (outside RedisStore, use raw client) ──
// TODO: Wire into reconnect handler.

/// Create a raw Redis client for pub/sub (outside connection pool).
/// Pub/sub requires a dedicated connection that cannot be shared via a pool.
#[allow(dead_code)]
pub fn create_pubsub_client(redis_url: &str) -> anyhow::Result<redis::Client> {
    Ok(redis::Client::open(redis_url)?)
}

/// Parse an expired key event to extract a worker_id.
/// Key format: `{prefix}worker:alive:{worker_id}`
/// Returns `Some(worker_id)` if the key matches, `None` otherwise.
#[allow(dead_code)]
pub fn parse_worker_death_key(key: &str, prefix: &str) -> Option<String> {
    let pattern = format!("{}worker:alive:", prefix);
    key.strip_prefix(&pattern).map(|s| s.to_string())
}

/// Subscribe to worker death events via Redis keyspace notifications.
///
/// Returns a receiver of worker_ids whose presence keys have expired.
/// **Prerequisite:** Redis must have keyspace notifications enabled:
/// `CONFIG SET notify-keyspace-events Ex`
///
/// This function spawns a background task that listens on a dedicated
/// pub/sub connection (not from the pool) and forwards parsed worker_ids
/// to the returned channel.
#[allow(dead_code)]
pub async fn subscribe_worker_deaths(
    redis_url: &str,
    prefix: String,
) -> anyhow::Result<tokio::sync::mpsc::Receiver<String>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(32);
    let client = create_pubsub_client(redis_url)?;

    tokio::spawn(async move {
        let mut pubsub = match client.get_async_pubsub().await {
            Ok(ps) => ps,
            Err(e) => {
                tracing::error!("Failed to get async pubsub connection: {}", e);
                return;
            }
        };

        // Subscribe to expired key events on database 0.
        let channel = "__keyevent@0__:expired";
        if let Err(e) = pubsub.subscribe(channel).await {
            tracing::error!("Failed to subscribe to {}: {}", channel, e);
            return;
        }
        tracing::info!(
            "Subscribed to Redis keyspace expired events on channel: {}",
            channel
        );

        let mut msg_stream = pubsub.on_message();

        loop {
            let msg = match msg_stream.next().await {
                Some(m) => m,
                None => {
                    tracing::warn!("Redis pubsub stream ended, stopping worker death listener");
                    break;
                }
            };

            let expired_key: String = match msg.get_payload() {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!("Failed to parse pubsub payload: {}", e);
                    continue;
                }
            };

            if let Some(worker_id) = parse_worker_death_key(&expired_key, &prefix) {
                tracing::warn!("Worker death detected via key expiry: {}", worker_id);
                if tx.send(worker_id).await.is_err() {
                    tracing::info!("Worker death receiver dropped, stopping listener");
                    break;
                }
            }
        }
    });

    Ok(rx)
}
