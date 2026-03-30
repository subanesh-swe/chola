use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

/// GET /api/v1/job-groups/:id/jobs
pub async fn list_by_group(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let jobs = storage
        .get_jobs_for_group(group_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let list: Vec<Value> = jobs
        .iter()
        .map(|j| {
            json!({
                "id": j.id.to_string(),
                "job_group_id": j.job_group_id.to_string(),
                "stage_name": j.stage_name,
                "command": j.command,
                "worker_id": j.worker_id,
                "state": j.state,
                "exit_code": j.exit_code,
                "created_at": j.created_at.to_rfc3339(),
                "updated_at": j.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "jobs": list, "count": list.len() })))
}

/// GET /api/v1/jobs/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let job = storage
        .get_job(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Job not found".into()))?;

    Ok(Json(json!({
        "id": job.id.to_string(),
        "job_group_id": job.job_group_id.to_string(),
        "stage_config_id": job.stage_config_id.to_string(),
        "stage_name": job.stage_name,
        "command": job.command,
        "pre_script": job.pre_script,
        "post_script": job.post_script,
        "worker_id": job.worker_id,
        "state": job.state,
        "exit_code": job.exit_code,
        "created_at": job.created_at.to_rfc3339(),
        "updated_at": job.updated_at.to_rfc3339(),
    })))
}

/// POST /api/v1/jobs/:id/cancel
pub async fn cancel(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_cancel_jobs() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    // Try in-memory registry first (job_id is a string in the registry)
    let job_id_str = id.to_string();
    let worker_id = {
        let mut registry = state.job_registry.write().await;
        registry.cancel_job(&job_id_str, "Cancelled via API")
    };

    // Also update in DB
    if let Some(storage) = &state.storage {
        let _ = storage
            .update_job_state(id, "cancelled", None, None, None, None)
            .await;
    }

    match worker_id {
        Some(wid) => Ok(Json(json!({
            "id": id.to_string(),
            "state": "cancelling",
            "worker_id": wid,
        }))),
        None => Ok(Json(json!({
            "id": id.to_string(),
            "state": "cancelled",
        }))),
    }
}
