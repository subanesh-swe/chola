use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use ci_core::models::user::UserRole;

use super::jwt;

/// Extracted from JWT token or API key on every protected request.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: UserRole,
}

/// State needed by the auth extractor.
#[derive(Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub jwt_secret: String,
    /// Storage reference for session revocation + API key lookups.
    pub storage: Option<Arc<crate::storage::Storage>>,
}

impl AuthConfig {
    pub fn from_controller_config(config: &ci_core::models::config::AuthConfig) -> Self {
        Self {
            enabled: config.enabled,
            jwt_secret: config.jwt_secret.clone(),
            storage: None,
        }
    }
}

/// Error returned when auth fails.
pub struct AuthError {
    status: StatusCode,
    message: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}

fn sha256_hex(data: &str) -> String {
    let mut h = Sha256::new();
    h.update(data.as_bytes());
    hex::encode(h.finalize())
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync + AsRef<AuthConfig>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_config = state.as_ref();

        if !auth_config.enabled {
            return Ok(AuthUser {
                user_id: Uuid::nil(),
                username: "anonymous".to_string(),
                role: UserRole::SuperAdmin,
            });
        }

        // --- API key / runner token: X-API-Key header ---
        if let Some(raw_key) = parts.headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
            if raw_key.starts_with("chola_svc_") {
                return resolve_runner_token(raw_key, auth_config).await;
            }
            return resolve_api_key(raw_key, auth_config).await;
        }

        // --- Query param token (for SSE/EventSource which can't set headers) ---
        let query_token = parts
            .uri
            .query()
            .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("token=")))
            .map(|t| t.to_string());

        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .or_else(|| query_token.as_ref().map(|t| format!("Bearer {t}")))
            .ok_or(AuthError {
                status: StatusCode::UNAUTHORIZED,
                message: "Missing Authorization header".to_string(),
            })?;
        let auth_header = auth_header.as_str();

        // --- Runner token: Authorization: Bearer chola_svc_* ---
        if let Some(rest) = auth_header.strip_prefix("Bearer chola_svc_") {
            let full = format!("chola_svc_{rest}");
            return resolve_runner_token(&full, auth_config).await;
        }

        // --- API key: Authorization: Bearer chola_* ---
        if let Some(raw_key) = auth_header.strip_prefix("Bearer chola_") {
            let full = format!("chola_{raw_key}");
            return resolve_api_key(&full, auth_config).await;
        }

        // --- JWT ---
        let token = auth_header.strip_prefix("Bearer ").ok_or(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid Authorization header format".to_string(),
        })?;

        let claims = jwt::decode_token(&auth_config.jwt_secret, token).map_err(|e| AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: format!("Invalid token: {e}"),
        })?;

        if let Some(storage) = &auth_config.storage {
            let valid = storage.is_session_valid(&claims.jti).await.unwrap_or(false);
            if !valid {
                return Err(AuthError {
                    status: StatusCode::UNAUTHORIZED,
                    message: "Session revoked or expired".to_string(),
                });
            }
        }

        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid user ID in token".to_string(),
        })?;

        Ok(AuthUser {
            user_id,
            username: claims.username,
            role: UserRole::from_db_str(&claims.role),
        })
    }
}

/// Resolve a runner token (chola_svc_* prefix) against the worker_tokens table.
/// Runner tokens are service accounts with operator-level access.
async fn resolve_runner_token(raw_key: &str, cfg: &AuthConfig) -> Result<AuthUser, AuthError> {
    let storage = cfg.storage.as_ref().ok_or(AuthError {
        status: StatusCode::UNAUTHORIZED,
        message: "Storage unavailable".to_string(),
    })?;

    let hash = sha256_hex(raw_key);
    let token = storage
        .get_worker_token_by_hash(&hash)
        .await
        .map_err(|_| AuthError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Token lookup failed".to_string(),
        })?
        .ok_or(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid runner token".to_string(),
        })?;

    if !token.active {
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Runner token is inactive".to_string(),
        });
    }
    if token.scope != "runner" {
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid runner token".to_string(),
        });
    }
    if let Some(exp) = token.expires_at {
        if exp < chrono::Utc::now() {
            return Err(AuthError {
                status: StatusCode::UNAUTHORIZED,
                message: "Runner token has expired".to_string(),
            });
        }
    }

    Ok(AuthUser {
        user_id: Uuid::nil(),
        username: format!("runner:{}", token.name),
        role: UserRole::Operator,
    })
}

async fn resolve_api_key(raw_key: &str, cfg: &AuthConfig) -> Result<AuthUser, AuthError> {
    let storage = cfg.storage.as_ref().ok_or(AuthError {
        status: StatusCode::UNAUTHORIZED,
        message: "Storage unavailable".to_string(),
    })?;

    let hash = sha256_hex(raw_key);
    let api_key = storage
        .get_api_key_by_hash(&hash)
        .await
        .map_err(|_| AuthError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Key lookup failed".to_string(),
        })?
        .ok_or(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid API key".to_string(),
        })?;

    let user = storage
        .get_user(api_key.user_id)
        .await
        .map_err(|_| AuthError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "User lookup failed".to_string(),
        })?
        .ok_or(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "API key owner not found".to_string(),
        })?;

    if !user.active {
        return Err(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Account is disabled".to_string(),
        });
    }

    Ok(AuthUser {
        user_id: user.id,
        username: user.username,
        role: user.role,
    })
}
