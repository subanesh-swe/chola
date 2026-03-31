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
pub struct CreateNotificationRequest {
    pub trigger: String,
    pub channel_type: String,
    pub config: Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct UpdateNotificationRequest {
    pub enabled: bool,
    pub config: Value,
}

fn validate_trigger(trigger: &str) -> Result<(), ApiError> {
    match trigger {
        "on_success" | "on_failure" | "on_complete" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "trigger must be on_success, on_failure, or on_complete".into(),
        )),
    }
}

fn validate_channel_type(channel_type: &str) -> Result<(), ApiError> {
    match channel_type {
        "slack" | "webhook" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "channel_type must be slack or webhook".into(),
        )),
    }
}

/// GET /api/v1/repos/{id}/notifications
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let configs = storage
        .list_notification_configs(repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(
        json!({ "notifications": configs, "count": configs.len() }),
    ))
}

/// POST /api/v1/repos/{id}/notifications
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<CreateNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    validate_trigger(&body.trigger)?;
    validate_channel_type(&body.channel_type)?;
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let cfg = storage
        .create_notification_config(
            repo_id,
            &body.trigger,
            &body.channel_type,
            body.config,
            body.enabled,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!(cfg)))
}

/// PUT /api/v1/repos/{id}/notifications/{nid}
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, nid)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateNotificationRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let cfg = storage
        .update_notification_config(nid, body.enabled, body.config)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Notification config not found".into()))?;
    Ok(Json(json!(cfg)))
}

/// DELETE /api/v1/repos/{id}/notifications/{nid}
pub async fn delete(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, nid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_notification_config(nid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({ "deleted": true })))
    } else {
        Err(ApiError::NotFound("Notification config not found".into()))
    }
}
