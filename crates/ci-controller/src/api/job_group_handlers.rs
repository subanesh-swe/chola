use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;
use uuid::Uuid;

use ci_core::models::job_group::JobGroupState;
use ci_core::proto::orchestrator::ReserveWorkerRequest;

use crate::auth::middleware::AuthUser;
use crate::grpc_server::do_reserve_worker;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Query params ─────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub state: Option<String>,
    pub repo_id: Option<Uuid>,
}

// ── Request bodies ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TriggerRequest {
    pub repo_name: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub stages: Option<Vec<String>>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/v1/job-groups
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (groups, total) = storage
        .list_job_groups_paginated(limit, offset, params.state.as_deref(), params.repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let list: Vec<Value> = groups
        .iter()
        .map(|g| {
            json!({
                "id": g.id.to_string(),
                "repo_id": g.repo_id.to_string(),
                "branch": g.branch,
                "commit_sha": g.commit_sha,
                "trigger_source": g.trigger_source,
                "reserved_worker_id": g.reserved_worker_id,
                "state": g.state.to_string(),
                "created_at": g.created_at.to_rfc3339(),
                "updated_at": g.updated_at.to_rfc3339(),
                "completed_at": g.completed_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": list,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
}

/// GET /api/v1/job-groups/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let (group, jobs) = storage
        .get_job_group_with_jobs(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Job group not found".into()))?;

    let job_list: Vec<Value> = jobs
        .iter()
        .map(|j| {
            json!({
                "id": j.id.to_string(),
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

    Ok(Json(json!({
        "id": group.id.to_string(),
        "repo_id": group.repo_id.to_string(),
        "branch": group.branch,
        "commit_sha": group.commit_sha,
        "trigger_source": group.trigger_source,
        "reserved_worker_id": group.reserved_worker_id,
        "state": group.state.to_string(),
        "created_at": group.created_at.to_rfc3339(),
        "updated_at": group.updated_at.to_rfc3339(),
        "completed_at": group.completed_at.map(|t| t.to_rfc3339()),
        "jobs": job_list,
    })))
}

/// POST /api/v1/job-groups/:id/cancel
pub async fn cancel(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_cancel_jobs() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    // Update in-memory registry
    {
        let mut jg = state.job_group_registry.write().await;
        if let Some(group) = jg.get(&id) {
            if group.state.is_terminal() {
                return Err(ApiError::Conflict(
                    "Job group already in terminal state".into(),
                ));
            }
        }
        jg.update_state(&id, JobGroupState::Cancelled);
        jg.fail_group_jobs(&id, "Cancelled via API");
    }

    // Update in DB
    if let Some(storage) = &state.storage {
        let _ = storage
            .update_job_group_state(id, JobGroupState::Cancelled)
            .await;
    }

    Ok(Json(json!({"id": id.to_string(), "state": "cancelled"})))
}

/// POST /api/v1/job-groups  — trigger a new build from REST
pub async fn trigger(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<TriggerRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_trigger_builds() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    let req = ReserveWorkerRequest {
        repo_name: body.repo_name.clone(),
        repo_url: String::new(),
        branch: body.branch.unwrap_or_default(),
        commit_sha: body.commit_sha.unwrap_or_default(),
        stages: body.stages.unwrap_or_default(),
    };

    let resp = do_reserve_worker(&state, &req)
        .await
        .map_err(|e| ApiError::Internal(e.message().to_string()))?;

    if !resp.success {
        return Err(ApiError::Conflict(resp.message));
    }

    let stages: Vec<Value> = resp
        .stages
        .iter()
        .map(|s| json!({"stage_name": s.stage_name}))
        .collect();

    Ok(Json(json!({
        "job_group_id": resp.job_group_id,
        "worker_id": resp.worker_id,
        "stages": stages,
        "message": resp.message,
    })))
}

/// POST /api/v1/job-groups/:id/retry  — re-run a failed/cancelled build
pub async fn retry(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_trigger_builds() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Look up original group
    let (original, original_jobs) = storage
        .get_job_group_with_jobs(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Job group not found".into()))?;

    if !original.state.is_terminal() {
        return Err(ApiError::Conflict(
            "Can only retry terminal (failed/cancelled/success) builds".into(),
        ));
    }

    // Look up repo name from repo_id
    let repo = storage
        .get_repo(original.repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Repo no longer exists".into()))?;

    // Collect stage names from original jobs
    let stages: Vec<String> = original_jobs.iter().map(|j| j.stage_name.clone()).collect();

    let req = ReserveWorkerRequest {
        repo_name: repo.repo_name,
        repo_url: repo.repo_url,
        branch: original.branch.unwrap_or_default(),
        commit_sha: original.commit_sha.unwrap_or_default(),
        stages,
    };

    let resp = do_reserve_worker(&state, &req)
        .await
        .map_err(|e| ApiError::Internal(e.message().to_string()))?;

    if !resp.success {
        warn!("Retry of group {} failed: {}", id, resp.message);
        return Err(ApiError::Conflict(resp.message));
    }

    Ok(Json(json!({
        "job_group_id": resp.job_group_id,
        "worker_id": resp.worker_id,
        "original_group_id": id.to_string(),
        "message": "Build re-triggered successfully",
    })))
}
