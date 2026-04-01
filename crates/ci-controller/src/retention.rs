use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::state::ControllerState;
use ci_core::models::config::RetentionConfig;

pub fn spawn_cleanup_task(
    state: Arc<ControllerState>,
    config: RetentionConfig,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval_secs = config.cleanup_interval_secs.max(60);
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        interval.tick().await; // skip first immediate tick

        info!(
            max_age_days = config.max_age_days,
            max_per_repo = config.max_builds_per_repo,
            "Retention cleanup started"
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Retention cleanup shutting down");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = run_cleanup(&state, &config).await {
                        warn!(error = %e, "Retention cleanup failed");
                    }
                }
            }
        }
    })
}

async fn run_cleanup(state: &ControllerState, config: &RetentionConfig) -> anyhow::Result<()> {
    let storage = state
        .storage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No storage"))?;
    let mut total = 0u64;

    if config.max_age_days > 0 {
        let expired = storage
            .find_expired_groups(config.max_age_days as i32)
            .await?;
        if !expired.is_empty() {
            let count = expired.len();
            // Delete DB rows first (CASCADE handles child tables)
            for batch in expired.chunks(100) {
                total += storage.delete_job_groups_batch(batch).await?;
                tokio::task::yield_now().await;
            }
            // Then clean up log dirs (best-effort)
            if let Some(log_dir) = &state.config.logging.log_dir {
                for id in &expired {
                    let path = std::path::Path::new(log_dir).join(id.to_string());
                    if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            warn!(group_id = %id, error = %e, "Failed to remove log dir");
                        }
                    }
                }
            }
            info!(count, "Cleaned up expired groups");
        }
    }

    if config.max_builds_per_repo > 0 {
        let excess = storage
            .find_excess_groups_per_repo(config.max_builds_per_repo as i32)
            .await?;
        if !excess.is_empty() {
            for batch in excess.chunks(100) {
                total += storage.delete_job_groups_batch(batch).await?;
                tokio::task::yield_now().await;
            }
            info!(count = excess.len(), "Cleaned up excess groups");
        }
    }

    if total > 0 {
        info!(total, "Retention cleanup completed");
    }
    Ok(())
}
