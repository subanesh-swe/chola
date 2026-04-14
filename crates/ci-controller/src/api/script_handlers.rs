use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use ci_core::models::stage::StageScript;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// -- Request types -----------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateScriptRequest {
    pub script_type: String,
    pub script_scope: String,
    pub script: String,
    pub worker_id: Option<String>,
    #[serde(default)]
    pub lock_enabled: bool,
    pub lock_key: Option<String>,
    #[serde(default = "default_lock_timeout")]
    pub lock_timeout_secs: i32,
}

fn default_lock_timeout() -> i32 {
    120
}

#[derive(Deserialize)]
pub struct UpdateScriptRequest {
    pub script_type: Option<String>,
    pub script_scope: Option<String>,
    pub script: Option<String>,
    /// Send `null` to clear, omit field to leave unchanged.
    pub worker_id: Option<Option<String>>,
    pub lock_enabled: Option<bool>,
    pub lock_key: Option<Option<String>>,
    pub lock_timeout_secs: Option<i32>,
}

// -- Validation --------------------------------------------------------------

fn validate_script_type(val: &str) -> Result<(), ApiError> {
    match val {
        "pre" | "post" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "script_type must be 'pre' or 'post'".into(),
        )),
    }
}

fn validate_script_scope(val: &str) -> Result<(), ApiError> {
    match val {
        "worker" | "master" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "script_scope must be 'worker' or 'master'".into(),
        )),
    }
}

// -- Helpers -----------------------------------------------------------------

fn script_to_json(s: &StageScript) -> Value {
    json!({
        "id": s.id.to_string(),
        "stage_config_id": s.stage_config_id.to_string(),
        "worker_id": s.worker_id,
        "script_type": s.script_type,
        "script_scope": s.script_scope,
        "script": s.script,
        "lock_enabled": s.lock_enabled,
        "lock_key": s.lock_key,
        "lock_timeout_secs": s.lock_timeout_secs,
        "created_at": s.created_at.to_rfc3339(),
        "updated_at": s.updated_at.to_rfc3339(),
    })
}

// -- Handlers ----------------------------------------------------------------

/// GET /api/v1/repos/{repo_id}/stages/{stage_id}/scripts
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path((_repo_id, stage_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let scripts = storage
        .list_stage_scripts(stage_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let list: Vec<Value> = scripts.iter().map(script_to_json).collect();
    Ok(Json(json!({ "scripts": list, "count": list.len() })))
}

/// POST /api/v1/repos/{repo_id}/stages/{stage_id}/scripts
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, stage_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CreateScriptRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    validate_script_type(&body.script_type)?;
    validate_script_scope(&body.script_scope)?;
    if body.script.is_empty() {
        return Err(ApiError::BadRequest("script cannot be empty".into()));
    }
    if body.script.len() > 65536 {
        return Err(ApiError::BadRequest("script must be under 64KB".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let script = storage
        .create_stage_script(
            stage_id,
            &body.script_type,
            &body.script_scope,
            &body.script,
            body.worker_id.as_deref(),
            body.lock_enabled,
            body.lock_key.as_deref(),
            body.lock_timeout_secs,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(script_to_json(&script)))
}

/// PUT /api/v1/repos/{repo_id}/stages/{stage_id}/scripts/{script_id}
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, _stage_id, script_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(body): Json<UpdateScriptRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if let Some(ref t) = body.script_type {
        validate_script_type(t)?;
    }
    if let Some(ref s) = body.script_scope {
        validate_script_scope(s)?;
    }
    if let Some(ref s) = body.script {
        if s.is_empty() {
            return Err(ApiError::BadRequest("script cannot be empty".into()));
        }
        if s.len() > 65536 {
            return Err(ApiError::BadRequest("script must be under 64KB".into()));
        }
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let worker_id = body.worker_id.as_ref().map(|opt| opt.as_deref());
    let lock_key = body.lock_key.as_ref().map(|opt| opt.as_deref());
    let script = storage
        .update_stage_script(
            script_id,
            body.script_type.as_deref(),
            body.script_scope.as_deref(),
            body.script.as_deref(),
            worker_id,
            body.lock_enabled,
            lock_key,
            body.lock_timeout_secs,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Script not found".into()))?;
    Ok(Json(script_to_json(&script)))
}

/// DELETE /api/v1/repos/{repo_id}/stages/{stage_id}/scripts/{script_id}
pub async fn delete_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, _stage_id, script_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_stage_script(script_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("Script not found".into()))
    }
}
