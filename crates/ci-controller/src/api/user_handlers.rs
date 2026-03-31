use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{middleware::AuthUser, password};
use crate::state::ControllerState;

use super::error::ApiError;

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_string(field: &str, value: &str, max_len: usize) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{} cannot be empty", field)));
    }
    if value.len() > max_len {
        return Err(ApiError::BadRequest(format!(
            "{} exceeds max length of {}",
            field, max_len
        )));
    }
    Ok(())
}

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub active: Option<bool>,
    pub password: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 8 {
        return Err(ApiError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }
    if !password.chars().any(|c| c.is_uppercase()) {
        return Err(ApiError::BadRequest(
            "Password must contain an uppercase letter".to_string(),
        ));
    }
    if !password.chars().any(|c| c.is_numeric()) {
        return Err(ApiError::BadRequest(
            "Password must contain a number".to_string(),
        ));
    }
    Ok(())
}

fn user_to_json(u: &ci_core::models::user::User) -> Value {
    json!({
        "id": u.id.to_string(),
        "username": u.username,
        "display_name": u.display_name,
        "role": u.role.to_string(),
        "active": u.active,
        "created_at": u.created_at.to_rfc3339(),
        "updated_at": u.updated_at.to_rfc3339(),
    })
}

// ── Query params ────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/v1/users
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_users() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (users, total) = storage
        .list_users_paginated(limit, offset)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let data: Vec<Value> = users.iter().map(user_to_json).collect();
    Ok(Json(json!({
        "data": data,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
}

/// POST /api/v1/users
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_users() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    validate_string("username", &body.username, 100)?;
    if let Some(ref dn) = body.display_name {
        validate_string("display_name", dn, 255)?;
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    validate_password(&body.password)?;
    let hash =
        password::hash_password(&body.password).map_err(|e| ApiError::Internal(e.to_string()))?;

    let user = storage
        .create_user(
            &body.username,
            &hash,
            body.display_name.as_deref(),
            &body.role,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(user_to_json(&user)))
}

/// GET /api/v1/users/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_users() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let user = storage
        .get_user(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("User not found".into()))?;
    Ok(Json(user_to_json(&user)))
}

/// PUT /api/v1/users/:id
pub async fn update(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_users() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if let Some(ref dn) = body.display_name {
        validate_string("display_name", dn, 255)?;
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let password_hash = match &body.password {
        Some(pw) => {
            validate_password(pw)?;
            Some(password::hash_password(pw).map_err(|e| ApiError::Internal(e.to_string()))?)
        }
        None => None,
    };

    let user = storage
        .update_user(
            id,
            body.display_name.as_deref(),
            body.role.as_deref(),
            body.active,
            password_hash.as_deref(),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

    Ok(Json(user_to_json(&user)))
}

/// DELETE /api/v1/users/:id
pub async fn delete_one(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_users() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_user(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("User not found".into()))
    }
}
