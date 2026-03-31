use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use ci_core::models::stage::{Repo, StageConfig};

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateRepoRequest {
    pub repo_name: String,
    pub repo_url: String,
    pub default_branch: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRepoRequest {
    pub repo_name: Option<String>,
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateStageRequest {
    pub stage_name: String,
    pub command: String,
    #[serde(default)]
    pub required_cpu: i32,
    #[serde(default)]
    pub required_memory_mb: i32,
    #[serde(default)]
    pub required_disk_mb: i32,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: i32,
    #[serde(default)]
    pub execution_order: i32,
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub allow_worker_migration: bool,
    #[serde(default = "default_job_type")]
    pub job_type: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_max_duration() -> i32 {
    3600
}
fn default_job_type() -> String {
    "common".to_string()
}

#[derive(Deserialize)]
pub struct UpdateStageRequest {
    pub stage_name: Option<String>,
    pub command: Option<String>,
    pub required_cpu: Option<i32>,
    pub required_memory_mb: Option<i32>,
    pub required_disk_mb: Option<i32>,
    pub max_duration_secs: Option<i32>,
    pub execution_order: Option<i32>,
    pub parallel_group: Option<String>,
    pub allow_worker_migration: Option<bool>,
    pub job_type: Option<String>,
    pub depends_on: Option<Vec<String>>,
}

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_string(field: &str, value: &str, max_len: usize) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{} cannot be empty", field)));
    }
    if value.len() > max_len {
        return Err(ApiError::BadRequest(format!(
            "{} exceeds max length of {}",
            field, max_len
        )));
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn repo_to_json(r: &Repo) -> Value {
    json!({
        "id": r.id.to_string(),
        "repo_name": r.repo_name,
        "repo_url": r.repo_url,
        "default_branch": r.default_branch,
        "enabled": r.enabled,
        "created_at": r.created_at.to_rfc3339(),
        "updated_at": r.updated_at.to_rfc3339(),
    })
}

fn stage_to_json(s: &StageConfig) -> Value {
    json!({
        "id": s.id.to_string(),
        "repo_id": s.repo_id.to_string(),
        "stage_name": s.stage_name,
        "command": s.command,
        "required_cpu": s.required_cpu,
        "required_memory_mb": s.required_memory_mb,
        "required_disk_mb": s.required_disk_mb,
        "max_duration_secs": s.max_duration_secs,
        "execution_order": s.execution_order,
        "parallel_group": s.parallel_group,
        "allow_worker_migration": s.allow_worker_migration,
        "job_type": s.job_type,
        "depends_on": s.depends_on,
        "created_at": s.created_at.to_rfc3339(),
        "updated_at": s.updated_at.to_rfc3339(),
    })
}

// ── Query params ────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Repo handlers ────────────────────────────────────────────────────────────

/// GET /api/v1/repos
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (repos, total) = storage
        .list_repos_paginated(limit, offset)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let data: Vec<Value> = repos.iter().map(repo_to_json).collect();
    Ok(Json(json!({
        "data": data,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
}

/// POST /api/v1/repos
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<CreateRepoRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    validate_string("repo_name", &body.repo_name, 255)?;
    validate_string("repo_url", &body.repo_url, 2048)?;
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let branch = body.default_branch.as_deref().unwrap_or("main");
    let repo = storage
        .create_repo(&body.repo_name, &body.repo_url, branch)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(repo_to_json(&repo)))
}

/// GET /api/v1/repos/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let repo = storage
        .get_repo(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Repo not found".into()))?;
    Ok(Json(repo_to_json(&repo)))
}

/// PUT /api/v1/repos/:id
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRepoRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if let Some(ref name) = body.repo_name {
        validate_string("repo_name", name, 255)?;
    }
    if let Some(ref url) = body.repo_url {
        validate_string("repo_url", url, 2048)?;
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let repo = storage
        .update_repo(
            id,
            body.repo_name.as_deref(),
            body.repo_url.as_deref(),
            body.default_branch.as_deref(),
            body.enabled,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Repo not found".into()))?;
    Ok(Json(repo_to_json(&repo)))
}

/// DELETE /api/v1/repos/:id
pub async fn delete_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_repo(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("Repo not found".into()))
    }
}

// ── Stage config handlers ────────────────────────────────────────────────────

/// GET /api/v1/repos/:id/stages
pub async fn list_stages(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let stages = storage
        .get_stage_configs_for_repo(repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let list: Vec<Value> = stages.iter().map(stage_to_json).collect();
    Ok(Json(json!({ "stages": list, "count": list.len() })))
}

/// POST /api/v1/repos/:id/stages
pub async fn create_stage(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<CreateStageRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    validate_string("stage_name", &body.stage_name, 255)?;
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let stage = storage
        .create_stage_config(
            repo_id,
            &body.stage_name,
            &body.command,
            body.required_cpu,
            body.required_memory_mb,
            body.required_disk_mb,
            body.max_duration_secs,
            body.execution_order,
            body.parallel_group.as_deref(),
            body.allow_worker_migration,
            &body.job_type,
            &body.depends_on,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(stage_to_json(&stage)))
}

/// PUT /api/v1/repos/:repo_id/stages/:stage_id
pub async fn update_stage(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, stage_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateStageRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if let Some(ref name) = body.stage_name {
        validate_string("stage_name", name, 255)?;
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let stage = storage
        .update_stage_config(
            stage_id,
            body.stage_name.as_deref(),
            body.command.as_deref(),
            body.required_cpu,
            body.required_memory_mb,
            body.required_disk_mb,
            body.max_duration_secs,
            body.execution_order,
            body.parallel_group.as_deref(),
            body.allow_worker_migration,
            body.job_type.as_deref(),
            body.depends_on.as_deref(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Stage config not found".into()))?;
    Ok(Json(stage_to_json(&stage)))
}

/// DELETE /api/v1/repos/:repo_id/stages/:stage_id
pub async fn delete_stage(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, stage_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_stage_config(stage_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("Stage config not found".into()))
    }
}
