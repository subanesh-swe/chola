use std::sync::Arc;

use axum::{
    extract::{Path, Query},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize, Default)]
pub struct ResourceQuery {
    pub limit: Option<i64>,
}

/// GET /api/v1/repos/{repo_id}/stages/{stage_id}/resources
pub async fn get_resources(
    axum::extract::State(state): axum::extract::State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path((_repo_id, stage_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<ResourceQuery>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(20).min(100);

    let history = storage
        .list_resource_history(stage_id, limit)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let recommendation = storage
        .get_resource_recommendations(stage_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "history": history,
        "recommendation": recommendation,
    })))
}
