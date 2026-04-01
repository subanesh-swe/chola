use std::sync::Arc;

use axum::{extract::State, Json};
use serde_json::{json, Value};
use tracing::warn;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;
use crate::storage::Storage;

use super::error::ApiError;

/// GET /api/v1/audit-log
pub async fn list_audit_logs(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let entries = storage
        .list_audit_logs(200)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let count = entries.len();
    Ok(Json(json!({ "entries": entries, "count": count })))
}

/// Log an auditable action to the database.
pub async fn audit_action(
    storage: &Arc<Storage>,
    user_id: Uuid,
    username: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) {
    if let Err(e) = storage
        .create_audit_log(
            Some(user_id),
            username,
            action,
            Some(resource_type),
            Some(resource_id),
            None,
            None,
        )
        .await
    {
        warn!("Failed to write audit log for {action} by {username}: {e}");
    }
}
