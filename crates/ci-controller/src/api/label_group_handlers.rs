use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize)]
pub struct LabelGroupRequest {
    pub name: String,
    pub match_labels: Vec<String>,
    pub env_vars: Option<serde_json::Value>,
    pub pre_script: Option<String>,
    pub max_concurrent_jobs: Option<i32>,
    pub capabilities: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateLabelGroupRequest {
    pub name: Option<String>,
    pub match_labels: Option<Vec<String>>,
    pub env_vars: Option<serde_json::Value>,
    pub pre_script: Option<String>,
    pub max_concurrent_jobs: Option<i32>,
    pub capabilities: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

/// POST /api/v1/label-groups
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<LabelGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name is required".into()));
    }

    let group = storage
        .create_label_group(
            &body.name,
            &body.match_labels,
            body.env_vars.as_ref(),
            body.pre_script.as_deref(),
            body.max_concurrent_jobs,
            body.capabilities.as_deref().unwrap_or(&[]),
            body.enabled.unwrap_or(true),
            body.priority,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(label_group_to_json(&group)))
}

/// GET /api/v1/label-groups
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let groups = storage
        .list_label_groups()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let data: Vec<Value> = groups.iter().map(label_group_to_json).collect();
    Ok(Json(json!({ "data": data })))
}

/// GET /api/v1/label-groups/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let group = storage
        .get_label_group(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Label group '{}' not found", id)))?;

    Ok(Json(label_group_to_json(&group)))
}

/// PUT /api/v1/label-groups/:id
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateLabelGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let group = storage
        .update_label_group(
            id,
            body.name.as_deref(),
            body.match_labels.as_deref(),
            body.env_vars.as_ref(),
            body.pre_script.as_deref(),
            body.max_concurrent_jobs,
            body.capabilities.as_deref(),
            body.enabled,
            body.priority,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Label group '{}' not found", id)))?;

    Ok(Json(label_group_to_json(&group)))
}

/// DELETE /api/v1/label-groups/:id
pub async fn delete_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    storage
        .delete_label_group(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({ "deleted": true })))
}

fn label_group_to_json(g: &crate::storage::DbLabelGroup) -> Value {
    json!({
        "id": g.id.to_string(),
        "name": g.name,
        "match_labels": g.match_labels,
        "env_vars": g.env_vars,
        "pre_script": g.pre_script,
        "max_concurrent_jobs": g.max_concurrent_jobs,
        "capabilities": g.capabilities,
        "enabled": g.enabled,
        "priority": g.priority,
        "created_at": g.created_at.to_rfc3339(),
        "updated_at": g.updated_at.to_rfc3339(),
    })
}
