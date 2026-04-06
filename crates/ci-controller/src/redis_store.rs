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

    // ── Worker Reservation (per-group, non-exclusive) ──
    //
    // Key format: worker:reservation:{worker_id}:{group_id}
    // Multiple groups can reserve the same worker concurrently.
    // The real gating mechanism is WorkerState.allocate() in memory.

    /// Record a per-group reservation for a worker. Non-exclusive: multiple
    /// groups can hold reservations on the same worker simultaneously.
    pub async fn reserve_worker(
        &self,
        worker_id: &str,
        group_id: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "reservation", worker_id, group_id]);
        let _: () = conn.set_ex(&key, "1", ttl_secs).await?;
        info!(
            "Recorded reservation: worker {} group {} (TTL {}s)",
            worker_id, group_id, ttl_secs
        );
        Ok(())
    }

    /// Release a specific per-group reservation for a worker.
    pub async fn release_worker_reservation(
        &self,
        worker_id: &str,
        group_id: &str,
    ) -> anyhow::Result<()> {
        let mut conn = self.get_conn().await?;
        let key = self.key(&["worker", "reservation", worker_id, group_id]);
        let _: () = conn.del(&key).await?;
        info!(
            "Released reservation: worker {} group {}",
            worker_id, group_id
        );
        Ok(())
    }

    /// Release ALL reservations for a given worker (used on worker death).
    pub async fn release_all_worker_reservations(&self, worker_id: &str) -> anyhow::Result<usize> {
        let mut conn = self.get_conn().await?;
        let pattern = self.key(&["worker", "reservation", worker_id, "*"]);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;
        let count = keys.len();
        for key in &keys {
            let _: () = conn.del(key).await?;
        }
        if count > 0 {
            info!(
                "Force-released {} reservations for worker {}",
                count, worker_id
            );
        }
        Ok(count)
    }

    /// Refresh the TTL on a per-group reservation key.
    pub async fn refresh_reservation_ttl(
        &self,
        worker_id: &str,
        group_id: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let key = self.key(&["worker", "reservation", worker_id, group_id]);
        let mut conn = self.pool.get().await?;
        let _: () = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(ttl_secs as i64)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    /// Get remaining TTL (seconds) on a per-group reservation.
    /// Returns None if key absent.
    pub async fn get_reservation_ttl(
        &self,
        worker_id: &str,
        group_id: &str,
    ) -> anyhow::Result<Option<i64>> {
        let key = self.key(&["worker", "reservation", worker_id, group_id]);
        let mut conn = self.pool.get().await?;
        let ttl: i64 = redis::cmd("TTL").arg(&key).query_async(&mut conn).await?;
        Ok(if ttl >= 0 { Some(ttl) } else { None })
    }

    /// List all group IDs that have an active reservation on a worker.
    pub async fn get_worker_reservations(&self, worker_id: &str) -> anyhow::Result<Vec<String>> {
        let mut conn = self.get_conn().await?;
        let pattern = self.key(&["worker", "reservation", worker_id, "*"]);
        let prefix = self.key(&["worker", "reservation", worker_id, ""]);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;
        let group_ids = keys
            .iter()
            .filter_map(|k| k.strip_prefix(&prefix).map(|s| s.to_string()))
            .collect();
        Ok(group_ids)
    }

    /// Scan all worker reservation keys.
    /// Returns vec of (worker_id, group_id) pairs.
    pub async fn scan_all_reservations(&self) -> anyhow::Result<Vec<(String, String)>> {
        let mut conn = self.get_conn().await?;
        let pattern = self.key(&["worker", "reservation", "*"]);
        let prefix = self.key(&["worker", "reservation", ""]);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;
        let mut results = Vec::new();
        for key in keys {
            // Key format: {prefix}worker:reservation:{worker_id}:{group_id}
            let suffix = match key.strip_prefix(&prefix) {
                Some(s) => s,
                None => continue,
            };
            // Split on first ':' to get worker_id and group_id
            if let Some((worker_id, group_id)) = suffix.split_once(':') {
                results.push((worker_id.to_string(), group_id.to_string()));
            }
        }
        Ok(results)
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
