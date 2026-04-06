use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

/// Tunable setting keys that can be overridden at runtime via DB.
const EDITABLE_KEYS: &[&str] = &[
    "scheduling.strategy",
    "scheduling.nvme_preference",
    "scheduling.branch_affinity",
    "workers.heartbeat_timeout_secs",
    "workers.reservation_timeout_secs",
    "workers.idle_timeout_secs",
    "workers.stall_timeout_secs",
    "logging.level",
    "logging.log_dir",
    "retention.max_age_days",
    "retention.max_builds_per_repo",
    "execution.work_dir",
    "execution.log_dir",
    "execution.repos_dir",
];

/// Resolve a value: DB override > config file value.
fn resolve(db: &HashMap<String, String>, key: &str, config_val: &str) -> (String, String) {
    if let Some(db_val) = db.get(key) {
        (db_val.clone(), "database".to_string())
    } else {
        (config_val.to_string(), "config".to_string())
    }
}

/// GET /api/v1/settings — merged view with source info.
pub async fn get_settings(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let cfg = &state.config;
    let db_settings = if let Some(s) = &state.storage {
        s.get_all_config_settings().await.unwrap_or_default()
    } else {
        HashMap::new()
    };

    let retention = cfg.retention.clone().unwrap_or_default();

    let (strategy, strategy_src) = resolve(
        &db_settings,
        "scheduling.strategy",
        &cfg.scheduling.strategy,
    );
    let (nvme, nvme_src) = resolve(
        &db_settings,
        "scheduling.nvme_preference",
        &cfg.scheduling.nvme_preference.to_string(),
    );
    let (affinity, affinity_src) = resolve(
        &db_settings,
        "scheduling.branch_affinity",
        &cfg.scheduling.branch_affinity.to_string(),
    );
    let (hb_timeout, hb_src) = resolve(
        &db_settings,
        "workers.heartbeat_timeout_secs",
        &cfg.workers.heartbeat_timeout_secs.to_string(),
    );
    let (res_timeout, res_src) = resolve(
        &db_settings,
        "workers.reservation_timeout_secs",
        &cfg.workers.reservation_timeout_secs.to_string(),
    );
    let (idle_timeout, idle_src) = resolve(
        &db_settings,
        "workers.idle_timeout_secs",
        &cfg.workers.idle_timeout_secs.to_string(),
    );
    let (stall_timeout, stall_src) = resolve(
        &db_settings,
        "workers.stall_timeout_secs",
        &cfg.workers.stall_timeout_secs.to_string(),
    );
    let (log_level, log_src) = resolve(&db_settings, "logging.level", &cfg.logging.level);
    let (ret_age, ret_age_src) = resolve(
        &db_settings,
        "retention.max_age_days",
        &retention.max_age_days.to_string(),
    );
    let (ret_builds, ret_builds_src) = resolve(
        &db_settings,
        "retention.max_builds_per_repo",
        &retention.max_builds_per_repo.to_string(),
    );

    // Controller log dir
    let ctrl_log_default = ci_core::models::config::chola_data_dir("controller/logs");
    let ctrl_log_dir_default = cfg.logging.log_dir.as_deref().unwrap_or(&ctrl_log_default);
    let (ctrl_log_dir, ctrl_log_dir_src) =
        resolve(&db_settings, "logging.log_dir", ctrl_log_dir_default);

    // Worker execution paths (defaults shown — workers override via their own YAML)
    let (work_dir, work_dir_src) = resolve(
        &db_settings,
        "execution.work_dir",
        &ci_core::models::config::chola_data_dir("worker/jobs"),
    );
    let (exec_log_dir, exec_log_dir_src) = resolve(
        &db_settings,
        "execution.log_dir",
        &ci_core::models::config::chola_data_dir("worker/logs"),
    );
    let (repos_dir, repos_dir_src) = resolve(
        &db_settings,
        "execution.repos_dir",
        &ci_core::models::config::chola_data_dir("worker/repos"),
    );
    Ok(Json(json!({
        "settings": [
            { "key": "scheduling.strategy", "value": strategy, "source": strategy_src, "editable": true, "options": ["best-fit", "round-robin"] },
            { "key": "scheduling.nvme_preference", "value": nvme, "source": nvme_src, "editable": true, "type": "bool" },
            { "key": "scheduling.branch_affinity", "value": affinity, "source": affinity_src, "editable": true, "type": "bool" },
            { "key": "workers.heartbeat_timeout_secs", "value": hb_timeout, "source": hb_src, "editable": true, "type": "int", "min": 5, "max": 300 },
            { "key": "workers.reservation_timeout_secs", "value": res_timeout, "source": res_src, "editable": true, "type": "int", "min": 60, "max": 86400 },
            { "key": "workers.idle_timeout_secs", "value": idle_timeout, "source": idle_src, "editable": true, "type": "int", "min": 60, "max": 86400, "description": "Fail reserved groups with no stage submitted after this many seconds" },
            { "key": "workers.stall_timeout_secs", "value": stall_timeout, "source": stall_src, "editable": true, "type": "int", "min": 60, "max": 86400, "description": "Fail running groups with no activity after this many seconds" },
            { "key": "logging.level", "value": log_level, "source": log_src, "editable": true, "options": ["trace", "debug", "info", "warn", "error"] },
            { "key": "retention.max_age_days", "value": ret_age, "source": ret_age_src, "editable": true, "type": "int", "min": 0, "max": 3650 },
            { "key": "retention.max_builds_per_repo", "value": ret_builds, "source": ret_builds_src, "editable": true, "type": "int", "min": 0, "max": 100000 },
            { "key": "logging.log_dir", "value": ctrl_log_dir, "source": ctrl_log_dir_src, "editable": true, "type": "path", "description": "Controller log directory" },
            { "key": "execution.work_dir", "value": work_dir, "source": work_dir_src, "editable": true, "type": "path", "description": "Worker job workspace base directory" },
            { "key": "execution.log_dir", "value": exec_log_dir, "source": exec_log_dir_src, "editable": true, "type": "path", "description": "Worker log directory" },
            { "key": "execution.repos_dir", "value": repos_dir, "source": repos_dir_src, "editable": true, "type": "path", "description": "Worker bare git repo cache directory" },
            { "key": "server.bind_address", "value": &cfg.bind_address, "source": "config", "editable": false },
            { "key": "server.http_port", "value": cfg.http_port, "source": "config", "editable": false },
            { "key": "auth.enabled", "value": cfg.auth.enabled, "source": "config", "editable": false },
            { "key": "auth.jwt_expiry_secs", "value": cfg.auth.jwt_expiry_secs, "source": "config", "editable": false },
            { "key": "workers.heartbeat_interval_secs", "value": cfg.workers.heartbeat_interval_secs, "source": "config", "editable": false },
        ]
    })))
}

/// PUT /api/v1/settings — update a runtime-tunable setting.
pub async fn update_setting(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let key = body["key"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("key is required".into()))?;
    let value = body["value"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("value is required".into()))?;

    if !EDITABLE_KEYS.contains(&key) {
        return Err(ApiError::BadRequest(format!(
            "Setting '{}' is not editable at runtime",
            key
        )));
    }

    validate_setting_value(key, value)?;

    storage
        .set_config_setting(key, value, None, &auth_user.username)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        json!({ "status": "updated", "key": key, "value": value }),
    ))
}

/// DELETE /api/v1/settings/{key} — revert to config file value.
pub async fn delete_setting(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Admin access required".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_config_setting(&key)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !deleted {
        return Err(ApiError::NotFound(
            "Setting not found in database (already using config default)".into(),
        ));
    }
    Ok(Json(json!({ "status": "reverted", "key": key })))
}

/// Validate setting value based on key type/constraints.
fn validate_setting_value(key: &str, value: &str) -> Result<(), ApiError> {
    match key {
        "scheduling.strategy" => {
            if !["best-fit", "round-robin"].contains(&value) {
                return Err(ApiError::BadRequest(
                    "strategy must be 'best-fit' or 'round-robin'".into(),
                ));
            }
        }
        "scheduling.nvme_preference" | "scheduling.branch_affinity" => {
            if !["true", "false"].contains(&value) {
                return Err(ApiError::BadRequest(format!(
                    "{} must be 'true' or 'false'",
                    key
                )));
            }
        }
        "workers.heartbeat_timeout_secs" => {
            validate_int_range(key, value, 5, 300)?;
        }
        "workers.reservation_timeout_secs" => {
            validate_int_range(key, value, 60, 86400)?;
        }
        "workers.idle_timeout_secs" => {
            validate_int_range(key, value, 60, 86400)?;
        }
        "workers.stall_timeout_secs" => {
            validate_int_range(key, value, 60, 86400)?;
        }
        "logging.level" => {
            if !["trace", "debug", "info", "warn", "error"].contains(&value) {
                return Err(ApiError::BadRequest(
                    "level must be one of: trace, debug, info, warn, error".into(),
                ));
            }
        }
        "retention.max_age_days" => {
            validate_int_range(key, value, 0, 3650)?;
        }
        "retention.max_builds_per_repo" => {
            validate_int_range(key, value, 0, 100000)?;
        }
        "logging.log_dir" | "execution.work_dir" | "execution.log_dir" | "execution.repos_dir" => {
            validate_path(key, value)?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_path(key: &str, value: &str) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(ApiError::BadRequest(format!("{} must not be empty", key)));
    }
    if !value.starts_with('/') {
        return Err(ApiError::BadRequest(format!(
            "{} must be an absolute path (start with /)",
            key
        )));
    }
    if value.contains("..") {
        return Err(ApiError::BadRequest(format!(
            "{} must not contain '..'",
            key
        )));
    }
    Ok(())
}

fn validate_int_range(key: &str, value: &str, min: i64, max: i64) -> Result<(), ApiError> {
    let n: i64 = value
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("{} must be an integer", key)))?;
    if n < min || n > max {
        return Err(ApiError::BadRequest(format!(
            "{} must be between {} and {}",
            key, min, max
        )));
    }
    Ok(())
}
