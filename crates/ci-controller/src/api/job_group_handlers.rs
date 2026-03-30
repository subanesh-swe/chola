use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use ci_core::models::job_group::JobGroupState;

use crate::auth::middleware::AuthUser;
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
        "job_groups": list,
        "total": total,
        "limit": limit,
        "offset": offset,
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
