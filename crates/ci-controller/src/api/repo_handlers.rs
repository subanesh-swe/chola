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
    pub global_pre_script: Option<String>,
    pub global_pre_script_scope: Option<String>,
    pub global_post_script: Option<String>,
    pub global_post_script_scope: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRepoRequest {
    pub repo_name: Option<String>,
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
    pub enabled: Option<bool>,
    pub max_concurrent_builds: Option<i32>,
    pub cancel_superseded: Option<bool>,
    pub global_pre_script: Option<Option<String>>,
    pub global_pre_script_scope: Option<String>,
    pub global_post_script: Option<Option<String>>,
    pub global_post_script_scope: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateStageRequest {
    pub stage_name: String,
    pub command: Option<String>,
    #[serde(default = "default_cpu")]
    pub required_cpu: i32,
    #[serde(default = "default_memory_mb")]
    pub required_memory_mb: i32,
    #[serde(default = "default_disk_mb")]
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
    #[serde(default)]
    pub required_labels: Vec<String>,
    #[serde(default = "default_command_mode")]
    pub command_mode: String,
}

fn default_cpu() -> i32 {
    1
}
fn default_memory_mb() -> i32 {
    512
}
fn default_disk_mb() -> i32 {
    256
}
fn default_max_duration() -> i32 {
    3600
}
fn default_job_type() -> String {
    "common".to_string()
}
fn default_command_mode() -> String {
    "fixed".to_string()
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
    pub required_labels: Option<Vec<String>>,
    pub command_mode: Option<String>,
}

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_resource_fields(
    cpu: i32,
    memory_mb: i32,
    disk_mb: i32,
    max_duration: i32,
) -> Result<(), ApiError> {
    if cpu < 0 || cpu > 1024 {
        return Err(ApiError::BadRequest("required_cpu must be 0-1024".into()));
    }
    if memory_mb < 0 || memory_mb > 1_048_576 {
        return Err(ApiError::BadRequest(
            "required_memory_mb must be 0-1048576".into(),
        ));
    }
    if disk_mb < 0 || disk_mb > 10_485_760 {
        return Err(ApiError::BadRequest(
            "required_disk_mb must be 0-10485760".into(),
        ));
    }
    if max_duration < 0 || max_duration > 86400 {
        return Err(ApiError::BadRequest(
            "max_duration_secs must be 0-86400".into(),
        ));
    }
    Ok(())
}

fn validate_command_mode(mode: &str) -> Result<(), ApiError> {
    match mode {
        "fixed" | "optional" | "required" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "command_mode must be one of: fixed, optional, required".into(),
        )),
    }
}

fn validate_script_scope(scope: &str) -> Result<(), ApiError> {
    match scope {
        "worker" | "master" | "both" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "script scope must be one of: worker, master, both".into(),
        )),
    }
}

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
        "max_concurrent_builds": r.max_concurrent_builds,
        "cancel_superseded": r.cancel_superseded,
        "global_pre_script": r.global_pre_script,
        "global_pre_script_scope": r.global_pre_script_scope,
        "global_post_script": r.global_post_script,
        "global_post_script_scope": r.global_post_script_scope,
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
        "command_mode": s.command_mode,
        "required_cpu": s.required_cpu,
        "required_memory_mb": s.required_memory_mb,
        "required_disk_mb": s.required_disk_mb,
        "max_duration_secs": s.max_duration_secs,
        "execution_order": s.execution_order,
        "parallel_group": s.parallel_group,
        "allow_worker_migration": s.allow_worker_migration,
        "job_type": s.job_type,
        "depends_on": s.depends_on,
        "required_labels": s.required_labels,
        "max_retries": s.max_retries,
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
#[utoipa::path(
    get,
    path = "/api/v1/repos",
    tag = "Repos",
    params(
        ("limit" = Option<i64>, Query, description = "Page size"),
        ("offset" = Option<i64>, Query, description = "Offset"),
    ),
    responses(
        (status = 200, description = "Paginated repo list"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
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
    if let Some(ref scope) = body.global_pre_script_scope {
        validate_script_scope(scope)?;
    }
    if let Some(ref scope) = body.global_post_script_scope {
        validate_script_scope(scope)?;
    }
    if let Some(ref s) = body.global_pre_script {
        if s.len() > 65536 {
            return Err(ApiError::BadRequest(
                "global_pre_script must be under 64KB".into(),
            ));
        }
    }
    if let Some(ref s) = body.global_post_script {
        if s.len() > 65536 {
            return Err(ApiError::BadRequest(
                "global_post_script must be under 64KB".into(),
            ));
        }
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let branch = body.default_branch.as_deref().unwrap_or("main");
    let repo = storage
        .create_repo(
            &body.repo_name,
            &body.repo_url,
            branch,
            body.global_pre_script.as_deref(),
            body.global_pre_script_scope.as_deref(),
            body.global_post_script.as_deref(),
            body.global_post_script_scope.as_deref(),
        )
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
    if let Some(ref scope) = body.global_pre_script_scope {
        validate_script_scope(scope)?;
    }
    if let Some(ref scope) = body.global_post_script_scope {
        validate_script_scope(scope)?;
    }
    if let Some(Some(ref s)) = body.global_pre_script {
        if s.len() > 65536 {
            return Err(ApiError::BadRequest(
                "global_pre_script must be under 64KB".into(),
            ));
        }
    }
    if let Some(Some(ref s)) = body.global_post_script {
        if s.len() > 65536 {
            return Err(ApiError::BadRequest(
                "global_post_script must be under 64KB".into(),
            ));
        }
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let repo = storage
        .update_repo(
            id,
            body.repo_name.as_deref(),
            body.repo_url.as_deref(),
            body.default_branch.as_deref(),
            body.enabled,
            body.max_concurrent_builds,
            body.cancel_superseded,
            body.global_pre_script.as_ref().map(|v| v.as_deref()),
            body.global_pre_script_scope.as_deref(),
            body.global_post_script.as_ref().map(|v| v.as_deref()),
            body.global_post_script_scope.as_deref(),
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
    validate_command_mode(&body.command_mode)?;
    // For fixed/optional modes, command should be provided
    if body.command_mode != "required" {
        if body.command.as_deref().unwrap_or("").is_empty() {
            return Err(ApiError::BadRequest(
                "command is required for fixed/optional mode".into(),
            ));
        }
    }
    validate_resource_fields(
        body.required_cpu,
        body.required_memory_mb,
        body.required_disk_mb,
        body.max_duration_secs,
    )?;
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let stage = storage
        .create_stage_config(
            repo_id,
            &body.stage_name,
            body.command.as_deref(),
            body.required_cpu,
            body.required_memory_mb,
            body.required_disk_mb,
            body.max_duration_secs,
            body.execution_order,
            body.parallel_group.as_deref(),
            body.allow_worker_migration,
            &body.job_type,
            Some(&body.depends_on[..]),
            Some(&body.required_labels[..]),
            &body.command_mode,
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
    if let Some(ref mode) = body.command_mode {
        validate_command_mode(mode)?;
    }
    // Validate resource fields when provided
    validate_resource_fields(
        body.required_cpu.unwrap_or(0),
        body.required_memory_mb.unwrap_or(0),
        body.required_disk_mb.unwrap_or(0),
        body.max_duration_secs.unwrap_or(3600),
    )?;
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
            body.required_labels.as_deref(),
            body.command_mode.as_deref(),
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
