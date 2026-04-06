use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub scope: Option<String>,
    pub expires_at: Option<String>,
    pub max_uses: Option<i32>,
}

fn sha256_hex(input: &str) -> String {
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

fn generate_registration_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "chola_reg_{}",
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    )
}

/// POST /api/v1/worker-tokens
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name is required".into()));
    }

    let token = generate_registration_token();
    let hash = sha256_hex(&token);
    let scope = body.scope.as_deref().unwrap_or("shared");
    let expires_at = body
        .expires_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let max_uses = body.max_uses.unwrap_or(0);

    let db_token = storage
        .create_worker_token(
            &body.name,
            &hash,
            scope,
            Some(auth_user.username.as_str()),
            expires_at,
            max_uses,
            None,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "id": db_token.id.to_string(),
        "name": db_token.name,
        "token": token,
        "scope": db_token.scope,
        "expires_at": db_token.expires_at.map(|t| t.to_rfc3339()),
        "max_uses": db_token.max_uses,
        "use_count": 0,
        "active": db_token.active,
        "created_by": db_token.created_by,
        "created_at": db_token.created_at.to_rfc3339(),
    })))
}

/// GET /api/v1/worker-tokens
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let tokens = storage
        .list_worker_tokens()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let data: Vec<Value> = tokens
        .iter()
        .map(|t| {
            json!({
                "id": t.id.to_string(),
                "name": t.name,
                "scope": t.scope,
                "expires_at": t.expires_at.map(|d| d.to_rfc3339()),
                "max_uses": t.max_uses,
                "use_count": t.uses,
                "active": t.active,
                "created_by": t.created_by,
                "created_at": t.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "data": data })))
}

/// PUT /api/v1/worker-tokens/:id/activate
pub async fn activate(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    set_active(state, auth_user, id, true).await
}

/// PUT /api/v1/worker-tokens/:id/deactivate
pub async fn deactivate(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    set_active(state, auth_user, id, false).await
}

async fn set_active(
    state: Arc<ControllerState>,
    auth_user: AuthUser,
    id: Uuid,
    active: bool,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    storage
        .update_worker_token_active(id, active)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({ "id": id.to_string(), "active": active })))
}

/// DELETE /api/v1/worker-tokens/:id
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
        .delete_worker_token(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({ "deleted": true })))
}
