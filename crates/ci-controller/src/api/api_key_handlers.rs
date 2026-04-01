use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

#[derive(Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Serialize)]
pub struct CreateApiKeyResponse {
    pub key: String, // returned once only
    #[serde(flatten)]
    pub meta: ApiKeyResponse,
}

fn generate_key() -> String {
    let bytes: [u8; 20] = rand::thread_rng().gen();
    format!("chola_{}", hex::encode(bytes))
}

fn sha256_hex(data: &str) -> String {
    let mut h = Sha256::new();
    h.update(data.as_bytes());
    hex::encode(h.finalize())
}

/// POST /api/v1/auth/api-keys
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
    if body.name.is_empty() || body.name.len() > 255 {
        return Err(ApiError::BadRequest("name must be 1–255 chars".to_string()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let raw_key = generate_key();
    let hash = sha256_hex(&raw_key);
    let api_key = storage
        .create_api_key(auth_user.user_id, &hash, &body.name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(CreateApiKeyResponse {
        key: raw_key,
        meta: ApiKeyResponse {
            id: api_key.id.to_string(),
            name: api_key.name,
            created_at: api_key.created_at.to_rfc3339(),
            last_used_at: api_key.last_used_at.map(|t| t.to_rfc3339()),
        },
    }))
}

/// GET /api/v1/auth/api-keys
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let keys = storage
        .list_api_keys_for_user(auth_user.user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let resp: Vec<ApiKeyResponse> = keys
        .into_iter()
        .map(|k| ApiKeyResponse {
            id: k.id.to_string(),
            name: k.name,
            created_at: k.created_at.to_rfc3339(),
            last_used_at: k.last_used_at.map(|t| t.to_rfc3339()),
        })
        .collect();

    Ok(Json(json!(resp)))
}

/// DELETE /api/v1/auth/api-keys/{id}
pub async fn revoke(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let revoked = storage
        .revoke_api_key(id, auth_user.user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if revoked {
        Ok(Json(json!({"message": "API key revoked"})))
    } else {
        Err(ApiError::NotFound("API key not found".to_string()))
    }
}
