use redis::aio::ConnectionManager;
use tracing::info;

/// Redis connection for distributed locks and monitoring
pub struct RedisStore {
    conn: ConnectionManager,
    prefix: String,
}

impl RedisStore {
    pub async fn new(redis_url: &str, prefix: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = ConnectionManager::new(client).await?;
        info!("Connected to Redis");
        Ok(Self {
            conn,
            prefix: prefix.to_string(),
        })
    }

    /// Acquire distributed lock for job assignment
    pub async fn acquire_job_lock(&mut self, job_id: &str) -> anyhow::Result<bool> {
        let key = format!("{}lock:job:{}", self.prefix, job_id);
        let result: Option<bool> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(30)
            .query_async(&mut self.conn)
            .await?;
        Ok(result.unwrap_or(false))
    }

    /// Release distributed lock
    pub async fn release_job_lock(&mut self, job_id: &str) -> anyhow::Result<()> {
        let key = format!("{}lock:job:{}", self.prefix, job_id);
        let _: () = redis::cmd("DEL")
            .arg(&key)
            .query_async(&mut self.conn)
            .await?;
        Ok(())
    }

    /// Update worker heartbeat in Redis
    pub async fn update_worker_heartbeat(&mut self, worker_id: &str) -> anyhow::Result<()> {
        let key = format!("{}worker:{}", self.prefix, worker_id);
        let _: () = redis::cmd("SETEX")
            .arg(&key)
            .arg(60)
            .arg(chrono::Utc::now().to_rfc3339())
            .query_async(&mut self.conn)
            .await?;
        Ok(())
    }
}
