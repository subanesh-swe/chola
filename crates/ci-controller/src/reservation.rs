use tracing::info;
use uuid::Uuid;

use crate::redis_store::RedisStore;

/// Manages worker reservations using Redis for per-group tracking.
///
/// Reservations are non-exclusive: multiple groups can share a worker
/// concurrently. The real capacity gate is `WorkerState::allocate()` in
/// memory; Redis tracks per-group keys for TTL-based expiry and audit.
pub struct ReservationManager;

impl ReservationManager {
    /// Record a per-group reservation in Redis.
    /// Does NOT gate on exclusivity -- caller must have already
    /// succeeded on `WorkerState::allocate()`.
    pub async fn reserve(
        redis: &RedisStore,
        worker_id: &str,
        job_group_id: &Uuid,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        redis
            .reserve_worker(worker_id, &job_group_id.to_string(), ttl_secs)
            .await?;
        info!("Worker {} reserved for group {}", worker_id, job_group_id);
        Ok(())
    }

    /// Release a specific per-group reservation.
    pub async fn release(
        redis: &RedisStore,
        worker_id: &str,
        job_group_id: &Uuid,
    ) -> anyhow::Result<()> {
        redis
            .release_worker_reservation(worker_id, &job_group_id.to_string())
            .await?;
        info!(
            "Worker {} reservation released (group {})",
            worker_id, job_group_id
        );
        Ok(())
    }

    /// Release ALL reservations for a worker (dead-worker cleanup).
    pub async fn release_force(redis: &RedisStore, worker_id: &str) -> anyhow::Result<()> {
        let count = redis.release_all_worker_reservations(worker_id).await?;
        info!(
            "Worker {} all reservations force-released ({})",
            worker_id, count
        );
        Ok(())
    }
}
