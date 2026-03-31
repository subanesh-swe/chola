use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use ci_core::models::variable::PipelineVariable;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateVariableRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Deserialize)]
pub struct UpdateVariableRequest {
    pub name: Option<String>,
    pub value: Option<String>,
    pub is_secret: Option<bool>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn variable_to_json(v: &PipelineVariable) -> Value {
    json!({
        "id": v.id.to_string(),
        "repo_id": v.repo_id.to_string(),
        "name": v.name,
        "value": if v.is_secret { "***".to_string() } else { v.value.clone() },
        "is_secret": v.is_secret,
        "created_at": v.created_at.to_rfc3339(),
        "updated_at": v.updated_at.to_rfc3339(),
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/v1/repos/:id/variables
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let vars = storage
        .list_variables_for_repo(repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let list: Vec<Value> = vars.iter().map(variable_to_json).collect();
    Ok(Json(json!({ "variables": list, "count": list.len() })))
}

/// POST /api/v1/repos/:id/variables
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<CreateVariableRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if body.name.is_empty() || body.name.len() > 255 {
        return Err(ApiError::BadRequest("name must be 1-255 characters".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let var = storage
        .create_variable(repo_id, &body.name, &body.value, body.is_secret)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(variable_to_json(&var)))
}

/// PUT /api/v1/repos/:repo_id/variables/:var_id
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, var_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateVariableRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if let Some(ref name) = body.name {
        if name.is_empty() || name.len() > 255 {
            return Err(ApiError::BadRequest("name must be 1-255 characters".into()));
        }
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let var = storage
        .update_variable(
            var_id,
            body.name.as_deref(),
            body.value.as_deref(),
            body.is_secret,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Variable not found".into()))?;
    Ok(Json(variable_to_json(&var)))
}

/// DELETE /api/v1/repos/:repo_id/variables/:var_id
pub async fn delete_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, var_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_variable(var_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("Variable not found".into()))
    }
}
