use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Query types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CommandBlacklistQuery {
    pub repo_id: Option<Uuid>,
    pub stage_config_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct BranchBlacklistQuery {
    pub worker_id: Option<String>,
}

// ── Command Blacklist Handlers ──────────────────────────────────────────────

/// GET /api/v1/blacklist/commands?repo_id=X&stage_config_id=Y
#[utoipa::path(
    get,
    path = "/api/v1/blacklist/commands",
    tag = "Blacklist",
    params(
        ("repo_id" = Option<uuid::Uuid>, Query, description = "Filter by repo"),
        ("stage_config_id" = Option<uuid::Uuid>, Query, description = "Filter by stage config"),
    ),
    responses(
        (status = 200, description = "Command blacklist entries"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_command_blacklist(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<CommandBlacklistQuery>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let items = storage
        .list_command_blacklist(params.repo_id, params.stage_config_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": items, "count": items.len() })))
}

/// POST /api/v1/blacklist/commands
pub async fn create_command_blacklist(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let pattern = body["pattern"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("pattern is required".into()))?;
    if pattern.is_empty() {
        return Err(ApiError::BadRequest("pattern cannot be empty".into()));
    }
    regex::Regex::new(pattern)
        .map_err(|e| ApiError::BadRequest(format!("Invalid regex: {}", e)))?;

    let repo_id = body["repo_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let stage_config_id = body["stage_config_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let description = body["description"].as_str();

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let id = storage
        .create_command_blacklist(repo_id, stage_config_id, pattern, description)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "id": id.to_string() })))
}

/// PUT /api/v1/blacklist/commands/{id}
pub async fn update_command_blacklist(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let pattern = body["pattern"].as_str();
    if let Some(p) = pattern {
        regex::Regex::new(p).map_err(|e| ApiError::BadRequest(format!("Invalid regex: {}", e)))?;
    }
    let description = body["description"].as_str();
    let enabled = body["enabled"].as_bool();

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let updated = storage
        .update_command_blacklist(id, pattern, description, enabled)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !updated {
        return Err(ApiError::NotFound("Blacklist entry not found".into()));
    }
    Ok(Json(json!({ "status": "updated" })))
}

/// DELETE /api/v1/blacklist/commands/{id}
pub async fn delete_command_blacklist(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_command_blacklist(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !deleted {
        return Err(ApiError::NotFound("Blacklist entry not found".into()));
    }
    Ok(Json(json!({ "status": "deleted" })))
}

// ── Branch Blacklist Handlers ───────────────────────────────────────────────

/// GET /api/v1/blacklist/branches?worker_id=X
pub async fn list_branch_blacklist(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<BranchBlacklistQuery>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let items = storage
        .list_branch_blacklist(params.worker_id.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": items, "count": items.len() })))
}

/// POST /api/v1/blacklist/branches
pub async fn create_branch_blacklist(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let worker_id = body["worker_id"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("worker_id is required".into()))?;
    if worker_id.is_empty() {
        return Err(ApiError::BadRequest("worker_id cannot be empty".into()));
    }
    let pattern = body["pattern"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("pattern is required".into()))?;
    if pattern.is_empty() {
        return Err(ApiError::BadRequest("pattern cannot be empty".into()));
    }
    regex::Regex::new(pattern)
        .map_err(|e| ApiError::BadRequest(format!("Invalid regex: {}", e)))?;

    let description = body["description"].as_str();

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let id = storage
        .create_branch_blacklist(worker_id, pattern, description)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "id": id.to_string() })))
}

/// PUT /api/v1/blacklist/branches/{id}
pub async fn update_branch_blacklist(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let pattern = body["pattern"].as_str();
    if let Some(p) = pattern {
        regex::Regex::new(p).map_err(|e| ApiError::BadRequest(format!("Invalid regex: {}", e)))?;
    }
    let description = body["description"].as_str();
    let enabled = body["enabled"].as_bool();

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let updated = storage
        .update_branch_blacklist(id, pattern, description, enabled)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !updated {
        return Err(ApiError::NotFound("Blacklist entry not found".into()));
    }
    Ok(Json(json!({ "status": "updated" })))
}

/// DELETE /api/v1/blacklist/branches/{id}
pub async fn delete_branch_blacklist(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_branch_blacklist(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !deleted {
        return Err(ApiError::NotFound("Blacklist entry not found".into()));
    }
    Ok(Json(json!({ "status": "deleted" })))
}
