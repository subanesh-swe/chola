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
    // Legacy single-rule retention. Retired by T5a (Wave 3); kept editable so
    // existing tooling can clear stale overrides during the rollout window.
    "retention.max_age_days",
    "retention.max_builds_per_repo",
    "retention.t1_purge_files_after_days",
    "retention.t2_archive_after_days",
    "retention.t3_delete_archive_after_days",
    "retention.cleanup_interval_secs",
    "retention.enable_worker_fanout",
    "execution.work_dir",
    "execution.log_dir",
    "execution.repos_dir",
];

/// Allowed range for the four numeric T1/T2/T3 retention keys.
const RETENTION_NUMERIC_BOUNDS: &[(&str, i64, i64)] = &[
    ("retention.max_builds_per_repo", 100, 1_000_000),
    ("retention.t1_purge_files_after_days", 1, 3_650),
    ("retention.t2_archive_after_days", 1, 3_650),
    ("retention.t3_delete_archive_after_days", 1, 36_500),
    ("retention.cleanup_interval_secs", 60, 86_400),
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
    let (ret_builds, ret_builds_src) = resolve(
        &db_settings,
        "retention.max_builds_per_repo",
        &retention.max_builds_per_repo.to_string(),
    );
    let (ret_t1, ret_t1_src) = resolve(
        &db_settings,
        "retention.t1_purge_files_after_days",
        &retention.t1_purge_files_after_days.to_string(),
    );
    let (ret_t2, ret_t2_src) = resolve(
        &db_settings,
        "retention.t2_archive_after_days",
        &retention.t2_archive_after_days.to_string(),
    );
    let (ret_t3, ret_t3_src) = resolve(
        &db_settings,
        "retention.t3_delete_archive_after_days",
        &retention.t3_delete_archive_after_days.to_string(),
    );
    let (ret_interval, ret_interval_src) = resolve(
        &db_settings,
        "retention.cleanup_interval_secs",
        &retention.cleanup_interval_secs.to_string(),
    );
    let (ret_fanout, ret_fanout_src) = resolve(
        &db_settings,
        "retention.enable_worker_fanout",
        &retention.enable_worker_fanout.to_string(),
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
            { "key": "retention.max_builds_per_repo", "value": ret_builds, "source": ret_builds_src, "editable": true, "type": "int", "min": 100, "max": 1000000, "description": "Hard cap on live job_groups rows per repo (safety net for runaway repos)" },
            { "key": "retention.t1_purge_files_after_days", "value": ret_t1, "source": ret_t1_src, "editable": true, "type": "int", "min": 1, "max": 3650, "description": "T1 — delete on-disk logs/workspace for terminal groups older than this" },
            { "key": "retention.t2_archive_after_days", "value": ret_t2, "source": ret_t2_src, "editable": true, "type": "int", "min": 1, "max": 3650, "description": "T2 — move DB rows to *_archive tables once group is older than this. Must be > T1." },
            { "key": "retention.t3_delete_archive_after_days", "value": ret_t3, "source": ret_t3_src, "editable": true, "type": "int", "min": 1, "max": 36500, "description": "T3 — hard-delete from *_archive. Must be > T2." },
            { "key": "retention.cleanup_interval_secs", "value": ret_interval, "source": ret_interval_src, "editable": true, "type": "int", "min": 60, "max": 86400, "description": "How often the retention loop runs" },
            { "key": "retention.enable_worker_fanout", "value": ret_fanout, "source": ret_fanout_src, "editable": true, "type": "bool", "description": "Push T1 purge directives to workers. Keep false until all workers are upgraded." },
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

    // Retention tier knobs: re-check t1 < t2 < t3 against the rest of the
    // currently-effective config (DB override > YAML fallback).
    if matches!(
        key,
        "retention.t1_purge_files_after_days"
            | "retention.t2_archive_after_days"
            | "retention.t3_delete_archive_after_days"
    ) {
        validate_retention_tier_ordering(&state, key, value).await?;
    }

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
        // Legacy single-rule knob — kept editable until T5a (Wave 3) removes it.
        "retention.max_age_days" => {
            validate_int_range(key, value, 0, 3650)?;
        }
        "retention.max_builds_per_repo"
        | "retention.t1_purge_files_after_days"
        | "retention.t2_archive_after_days"
        | "retention.t3_delete_archive_after_days"
        | "retention.cleanup_interval_secs" => {
            let (_, min, max) = RETENTION_NUMERIC_BOUNDS
                .iter()
                .find(|(k, _, _)| *k == key)
                .copied()
                .ok_or_else(|| ApiError::Internal(format!("no bounds for {}", key)))?;
            validate_int_range(key, value, min, max)?;
        }
        "retention.enable_worker_fanout" => {
            parse_bool(key, value)?;
        }
        "logging.log_dir" | "execution.work_dir" | "execution.log_dir" | "execution.repos_dir" => {
            validate_path(key, value)?;
        }
        _ => {}
    }
    Ok(())
}

/// Accept "true"/"false" (case-insensitive). JSON-bool inputs (`true`/`false`
/// without quotes) are unreachable here because the PUT handler extracts
/// `value` as `&str`; if a future caller relaxes that, this parser still
/// works on the string form.
fn parse_bool(key: &str, value: &str) -> Result<bool, ApiError> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ApiError::BadRequest(format!(
            "{} must be 'true' or 'false'",
            key
        ))),
    }
}

/// Re-check `t1 < t2 < t3` when one of the three tier knobs is being PUT.
/// Other two values are resolved from `config_settings` (DB override) with the
/// YAML fallback applied if no override exists.
async fn validate_retention_tier_ordering(
    state: &ControllerState,
    key: &str,
    value: &str,
) -> Result<(), ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let db = storage
        .get_all_config_settings()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let fallback = state.config.retention.clone().unwrap_or_default();

    let proposed: u64 = value
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("{} must be an integer", key)))?;

    let current = |k: &str, default: u32| -> Result<u64, ApiError> {
        match db.get(k) {
            Some(v) => v.parse::<u64>().map_err(|_| {
                ApiError::Internal(format!("stored {} is not an integer: {}", k, v))
            }),
            None => Ok(default as u64),
        }
    };

    let t1 = if key == "retention.t1_purge_files_after_days" {
        proposed
    } else {
        current(
            "retention.t1_purge_files_after_days",
            fallback.t1_purge_files_after_days,
        )?
    };
    let t2 = if key == "retention.t2_archive_after_days" {
        proposed
    } else {
        current(
            "retention.t2_archive_after_days",
            fallback.t2_archive_after_days,
        )?
    };
    let t3 = if key == "retention.t3_delete_archive_after_days" {
        proposed
    } else {
        current(
            "retention.t3_delete_archive_after_days",
            fallback.t3_delete_archive_after_days,
        )?
    };

    if t1 >= t2 {
        return Err(ApiError::BadRequest(format!(
            "retention tier ordering violated: t1 ({}) must be < t2 ({})",
            t1, t2
        )));
    }
    if t2 >= t3 {
        return Err(ApiError::BadRequest(format!(
            "retention tier ordering violated: t2 ({}) must be < t3 ({})",
            t2, t3
        )));
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
