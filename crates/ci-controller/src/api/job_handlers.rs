use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize, Default)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct RunListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub state: Option<String>,
    pub worker_id: Option<String>,
}

/// GET /api/v1/job-groups/:id/jobs
pub async fn list_by_group(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(group_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (jobs, total) = storage
        .get_jobs_for_group_paginated(group_id, limit, offset)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let data: Vec<Value> = jobs
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
                "status_reason": j.status_reason,
                "created_at": j.created_at.to_rfc3339(),
                "updated_at": j.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": data,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
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
        "stage_config_id": job.stage_config_id.map(|id| id.to_string()),
        "stage_name": job.stage_name,
        "command": job.command,
        "pre_script": job.pre_script,
        "post_script": job.post_script,
        "worker_id": job.worker_id,
        "state": job.state,
        "exit_code": job.exit_code,
        "status_reason": job.status_reason,
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
    let cancel_reason = format!("Cancelled via API by {}", auth_user.username);
    if let Some(storage) = &state.storage {
        if let Err(e) = storage
            .update_job_state(id, "cancelled", None, None, None, None)
            .await
        {
            warn!("Failed to persist cancel state for job {id}: {e}");
        }
        if let Err(e) = storage.update_job_reason(id, &cancel_reason).await {
            warn!("Failed to persist cancel reason for job {id}: {e}");
        }
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

/// POST /api/v1/jobs/:id/retry — manually re-queue a failed/cancelled job
pub async fn retry(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_trigger_builds() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let job = storage
        .retry_job(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("Job not found or not in a retryable state".into()))?;

    state.scheduler_notify.notify_waiters();

    Ok(Json(json!({
        "id": job.id.to_string(),
        "state": job.state,
        "retry_count": job.retry_count,
    })))
}

/// GET /api/v1/runs — individual job executions with group+repo context
pub async fn list_runs(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<RunListParams>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (runs, total) = storage
        .list_runs_paginated(
            limit,
            offset,
            params.state.as_deref(),
            params.worker_id.as_deref(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let data: Vec<Value> = runs
        .iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(),
                "job_group_id": r.job_group_id.to_string(),
                "stage_name": r.stage_name,
                "command": r.command,
                "worker_id": r.worker_id,
                "state": r.state,
                "exit_code": r.exit_code,
                "started_at": r.started_at.map(|t| t.to_rfc3339()),
                "completed_at": r.completed_at.map(|t| t.to_rfc3339()),
                "created_at": r.created_at.to_rfc3339(),
                "branch": r.branch,
                "repo_name": r.repo_name,
                "group_state": r.group_state,
                "trigger_source": r.trigger_source,
            })
        })
        .collect();

    Ok(Json(json!({
        "data": data,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
}
