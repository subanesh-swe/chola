use std::sync::Arc;

use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

/// GET /api/v1/settings
/// Returns a sanitized view of system configuration (no secrets).
pub async fn get_settings(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let cfg = &state.config;
    Ok(Json(json!({
        "auth": {
            "enabled": cfg.auth.enabled,
            "jwt_expiry_secs": cfg.auth.jwt_expiry_secs,
        },
        "scheduling": {
            "strategy": cfg.scheduling.strategy,
            "nvme_preference": cfg.scheduling.nvme_preference,
            "branch_affinity": cfg.scheduling.branch_affinity,
        },
        "workers": {
            "heartbeat_interval_secs": cfg.workers.heartbeat_interval_secs,
            "heartbeat_timeout_secs": cfg.workers.heartbeat_timeout_secs,
            "max_reconnect_attempts": cfg.workers.max_reconnect_attempts,
            "reservation_timeout_secs": cfg.workers.reservation_timeout_secs,
        },
        "logging": {
            "level": cfg.logging.level,
            "log_dir": cfg.logging.log_dir,
        },
    })))
}
