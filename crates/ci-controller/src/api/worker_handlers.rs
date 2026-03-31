use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize, Default)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /api/v1/workers
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let registry = state.worker_registry.read().await;
    let all = registry.all_workers();
    let total = all.len() as i64;

    let data: Vec<Value> = all
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|w| {
            let last_hb = w.last_heartbeat.as_ref().map(|hb| {
                json!({
                    "used_cpu_percent": hb.used_cpu_percent,
                    "used_memory_mb": hb.used_memory_mb,
                    "used_disk_mb": hb.used_disk_mb,
                    "running_jobs": hb.running_job_ids.len(),
                    "system_load": hb.system_load,
                    "timestamp": hb.timestamp.to_rfc3339(),
                })
            });
            json!({
                "worker_id": w.info.worker_id,
                "hostname": w.info.hostname,
                "status": format!("{:?}", w.status),
                "total_cpu": w.info.total_cpu,
                "total_memory_mb": w.info.total_memory_mb,
                "total_disk_mb": w.info.total_disk_mb,
                "disk_type": w.info.disk_type.to_string(),
                "docker_enabled": w.info.docker_enabled,
                "supported_job_types": w.info.supported_job_types,
                "registered_at": w.registered_at.to_rfc3339(),
                "last_heartbeat": last_hb,
            })
        })
        .collect();

    Ok(Json(json!({
        "data": data,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
}

/// GET /api/v1/workers/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let registry = state.worker_registry.read().await;
    let w = registry
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("Worker '{}' not found", id)))?;

    let last_hb = w.last_heartbeat.as_ref().map(|hb| {
        json!({
            "used_cpu_percent": hb.used_cpu_percent,
            "used_memory_mb": hb.used_memory_mb,
            "used_disk_mb": hb.used_disk_mb,
            "running_jobs": hb.running_job_ids.len(),
            "system_load": hb.system_load,
            "timestamp": hb.timestamp.to_rfc3339(),
        })
    });

    Ok(Json(json!({
        "worker_id": w.info.worker_id,
        "hostname": w.info.hostname,
        "status": format!("{:?}", w.status),
        "total_cpu": w.info.total_cpu,
        "total_memory_mb": w.info.total_memory_mb,
        "total_disk_mb": w.info.total_disk_mb,
        "disk_type": w.info.disk_type.to_string(),
        "docker_enabled": w.info.docker_enabled,
        "supported_job_types": w.info.supported_job_types,
        "registered_at": w.registered_at.to_rfc3339(),
        "last_heartbeat": last_hb,
    })))
}

/// POST /api/v1/workers/:id/drain
pub async fn drain(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let mut registry = state.worker_registry.write().await;
    if registry.mark_draining(&id) {
        info!("Worker {} set to drain mode via REST API", id);
        Ok(Json(json!({
            "worker_id": id,
            "status": "draining",
            "message": "Worker will finish current jobs then disconnect",
        })))
    } else {
        Err(ApiError::NotFound(format!("Worker '{}' not found", id)))
    }
}

/// POST /api/v1/workers/:id/undrain
pub async fn undrain(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let mut registry = state.worker_registry.write().await;
    let is_draining = registry
        .get(&id)
        .map(|w| w.status == ci_core::models::worker::WorkerStatus::Draining);

    match is_draining {
        Some(true) => {
            registry.mark_reconnected(&id);
            info!("Worker {} undrained via REST API", id);
            Ok(Json(json!({
                "worker_id": id,
                "status": "connected",
                "message": "Worker removed from drain mode",
            })))
        }
        Some(false) => Err(ApiError::Conflict(format!(
            "Worker '{}' is not in drain mode",
            id
        ))),
        None => Err(ApiError::NotFound(format!("Worker '{}' not found", id))),
    }
}
