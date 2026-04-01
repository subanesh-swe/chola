use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use ci_core::models::schedule::CronSchedule;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateScheduleRequest {
    pub interval_secs: i64,
    #[serde(default)]
    pub stages: Vec<String>,
    #[serde(default = "default_branch")]
    pub branch: String,
}

#[derive(Deserialize)]
pub struct UpdateScheduleRequest {
    pub interval_secs: Option<i64>,
    pub stages: Option<Vec<String>>,
    pub branch: Option<String>,
    pub enabled: Option<bool>,
}

fn default_branch() -> String {
    "main".to_string()
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn schedule_to_json(s: &CronSchedule) -> Value {
    json!({
        "id": s.id.to_string(),
        "repo_id": s.repo_id.to_string(),
        "interval_secs": s.interval_secs,
        "next_run_at": s.next_run_at.to_rfc3339(),
        "stages": s.stages,
        "branch": s.branch,
        "enabled": s.enabled,
        "last_triggered_at": s.last_triggered_at.map(|t| t.to_rfc3339()),
        "created_at": s.created_at.to_rfc3339(),
        "updated_at": s.updated_at.to_rfc3339(),
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/v1/repos/:id/schedules
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let schedules = storage
        .list_cron_schedules_for_repo(repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let list: Vec<Value> = schedules.iter().map(schedule_to_json).collect();
    Ok(Json(json!({ "schedules": list, "count": list.len() })))
}

/// POST /api/v1/repos/:id/schedules
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<CreateScheduleRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if body.interval_secs < 60 {
        return Err(ApiError::BadRequest(
            "interval_secs must be at least 60".into(),
        ));
    }
    if body.stages.is_empty() {
        return Err(ApiError::BadRequest("stages cannot be empty".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let schedule = storage
        .create_cron_schedule(repo_id, body.interval_secs, &body.stages, &body.branch)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(schedule_to_json(&schedule)))
}

/// PUT /api/v1/repos/:repo_id/schedules/:schedule_id
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, schedule_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateScheduleRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if let Some(secs) = body.interval_secs {
        if secs < 60 {
            return Err(ApiError::BadRequest(
                "interval_secs must be at least 60".into(),
            ));
        }
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let schedule = storage
        .update_cron_schedule(
            schedule_id,
            body.interval_secs,
            body.stages.as_deref(),
            body.branch.as_deref(),
            body.enabled,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Schedule not found".into()))?;
    Ok(Json(schedule_to_json(&schedule)))
}

/// DELETE /api/v1/repos/:repo_id/schedules/:schedule_id
pub async fn delete_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, schedule_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_cron_schedule(schedule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("Schedule not found".into()))
    }
}
