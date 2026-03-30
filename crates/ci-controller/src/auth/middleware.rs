use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use uuid::Uuid;

use ci_core::models::user::UserRole;

use super::jwt;

/// Extracted from JWT token on every protected request.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: UserRole,
}

/// State needed by the auth extractor.
/// This will be part of the app state.
#[derive(Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub jwt_secret: String,
    /// Storage reference for session revocation checks. Set after construction.
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

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync + AsRef<AuthConfig>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_config = state.as_ref();

        // If auth is disabled, return a synthetic super admin
        if !auth_config.enabled {
            return Ok(AuthUser {
                user_id: Uuid::nil(),
                username: "anonymous".to_string(),
                role: UserRole::SuperAdmin,
            });
        }

        // Extract the Authorization header
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError {
                status: StatusCode::UNAUTHORIZED,
                message: "Missing Authorization header".to_string(),
            })?;

        // Must be Bearer token
        let token = auth_header.strip_prefix("Bearer ").ok_or(AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid Authorization header format".to_string(),
        })?;

        // Decode the JWT (also validates expiry via jsonwebtoken)
        let claims = jwt::decode_token(&auth_config.jwt_secret, token).map_err(|e| AuthError {
            status: StatusCode::UNAUTHORIZED,
            message: format!("Invalid token: {}", e),
        })?;

        // Check session revocation in storage
        if let Some(storage) = &auth_config.storage {
            let valid = storage.is_session_valid(&claims.jti).await.unwrap_or(false);
            if !valid {
                return Err(AuthError {
                    status: StatusCode::UNAUTHORIZED,
                    message: "Session revoked or expired".to_string(),
                });
            }
        }

        // Parse user ID
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
