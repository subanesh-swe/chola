use tracing::{info, warn};
use uuid::Uuid;

use crate::redis_store::RedisStore;

/// Manages worker reservations using Redis for locking
pub struct ReservationManager;

impl ReservationManager {
    /// Attempt to reserve a worker for a job group
    pub async fn reserve(
        redis: &RedisStore,
        worker_id: &str,
        job_group_id: &Uuid,
        ttl_secs: u64,
    ) -> anyhow::Result<bool> {
        let acquired = redis
            .reserve_worker(worker_id, &job_group_id.to_string(), ttl_secs)
            .await?;
        if acquired {
            redis.remove_available_worker(worker_id).await?;
            info!("Worker {} reserved for group {}", worker_id, job_group_id);
        } else {
            warn!("Failed to reserve worker {} (already reserved)", worker_id);
        }
        Ok(acquired)
    }

    /// Release a worker reservation
    pub async fn release(redis: &RedisStore, worker_id: &str) -> anyhow::Result<()> {
        redis.release_worker(worker_id).await?;
        redis.add_available_worker(worker_id).await?;
        info!("Worker {} reservation released", worker_id);
        Ok(())
    }
}
