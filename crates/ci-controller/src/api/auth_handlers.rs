use std::sync::Arc;

use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use ci_core::models::user::User;

use crate::auth::{jwt, middleware::AuthUser, password};
use crate::state::ControllerState;

use super::error::ApiError;
use super::user_handlers::validate_password;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: String,
    pub user: UserResponse,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub role: String,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id.to_string(),
            username: u.username,
            display_name: u.display_name,
            role: u.role.to_string(),
            active: u.active,
            created_at: u.created_at.to_rfc3339(),
            updated_at: u.updated_at.to_rfc3339(),
        }
    }
}

/// POST /api/v1/auth/login
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 429, description = "Too many requests"),
    )
)]
pub async fn login(
    State(state): State<Arc<ControllerState>>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let user = storage
        .get_user_by_username(&body.username)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    if !user.active {
        return Err(ApiError::Unauthorized("Account is disabled".to_string()));
    }

    let valid = password::verify_password(&body.password, &user.password_hash)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    let jwt_secret = &state.config.auth.jwt_secret;
    let jwt_expiry = state.config.auth.jwt_expiry_secs;

    let (token, jti) = jwt::encode_token(jwt_secret, &user, jwt_expiry)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let expires_at = Utc::now() + chrono::Duration::seconds(jwt_expiry as i64);
    storage
        .create_session(user.id, &jti, expires_at)
        .await
        .map_err(|e| {
            warn!(
                "Failed to create session for user '{}': {}",
                user.username, e
            );
            ApiError::Internal("Failed to create session".to_string())
        })?;

    info!("User '{}' logged in", user.username);

    Ok(Json(LoginResponse {
        token,
        expires_at: expires_at.to_rfc3339(),
        user: user.into(),
    }))
}

/// PUT /api/v1/auth/password
#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let user = storage
        .get_user(auth_user.user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

    let valid = password::verify_password(&body.current_password, &user.password_hash)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !valid {
        return Err(ApiError::Unauthorized(
            "Current password is incorrect".into(),
        ));
    }

    validate_password(&body.new_password)?;
    let hash = password::hash_password(&body.new_password)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    storage
        .update_user_password(auth_user.user_id, &hash)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    info!("User '{}' changed their password", user.username);
    Ok(Json(json!({"message": "Password updated"})))
}

/// GET /api/v1/auth/me
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "Auth",
    responses(
        (status = 200, description = "Current user info"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn me(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let user = storage
        .get_user(auth_user.user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    let resp: UserResponse = user.into();
    Ok(Json(
        serde_json::to_value(resp).map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}

/// POST /api/v1/auth/logout
pub async fn logout(
    State(state): State<Arc<ControllerState>>,
    headers: axum::http::HeaderMap,
    _auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    if let Some(storage) = &state.storage {
        if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                if let Ok(claims) =
                    crate::auth::jwt::decode_token(&state.auth_config.jwt_secret, token)
                {
                    if let Err(e) = storage.revoke_session(&claims.jti).await {
                        tracing::warn!("Failed to revoke session: {}", e);
                    }
                }
            }
        }
    }
    Ok(Json(json!({"message": "Logged out"})))
}
