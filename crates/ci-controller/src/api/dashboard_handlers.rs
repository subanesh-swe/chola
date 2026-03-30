use std::sync::Arc;

use axum::{extract::State, Json};
use serde_json::{json, Value};

use ci_core::models::worker::WorkerStatus;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

/// GET /api/v1/dashboard/summary
pub async fn summary(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    // Worker stats from in-memory registry
    let (connected, disconnected, draining, total_workers) = {
        let wr = state.worker_registry.read().await;
        let all = wr.all_workers();
        let connected = all
            .iter()
            .filter(|w| w.status == WorkerStatus::Connected)
            .count();
        let disconnected = all
            .iter()
            .filter(|w| w.status == WorkerStatus::Disconnected)
            .count();
        let draining = all
            .iter()
            .filter(|w| w.status == WorkerStatus::Draining)
            .count();
        (connected, disconnected, draining, all.len())
    };

    // Job group stats from in-memory registry
    let (active_groups, running_groups) = {
        let jgr = state.job_group_registry.read().await;
        let active = jgr.active_groups();
        let running = active
            .iter()
            .filter(|g| g.state == ci_core::models::job_group::JobGroupState::Running)
            .count();
        (active.len(), running)
    };

    // Metrics counters
    let jobs_submitted = state
        .metrics
        .jobs_submitted
        .load(std::sync::atomic::Ordering::Relaxed);
    let jobs_completed = state
        .metrics
        .jobs_completed
        .load(std::sync::atomic::Ordering::Relaxed);
    let jobs_failed = state
        .metrics
        .jobs_failed
        .load(std::sync::atomic::Ordering::Relaxed);

    Ok(Json(json!({
        "workers": {
            "total": total_workers,
            "connected": connected,
            "disconnected": disconnected,
            "draining": draining,
        },
        "job_groups": {
            "active": active_groups,
            "running": running_groups,
        },
        "jobs": {
            "submitted": jobs_submitted,
            "completed": jobs_completed,
            "failed": jobs_failed,
        },
    })))
}
