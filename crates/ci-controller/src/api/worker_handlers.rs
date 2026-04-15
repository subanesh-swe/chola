use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
#[utoipa::path(
    get,
    path = "/api/v1/workers",
    tag = "Workers",
    params(
        ("limit" = Option<i64>, Query, description = "Page size"),
        ("offset" = Option<i64>, Query, description = "Offset"),
    ),
    responses(
        (status = 200, description = "Paginated worker list"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    // Merge: in-memory (live) + DB (registered but maybe offline)
    let jg_registry = state.job_group_registry.read().await;
    let registry = state.worker_registry.read().await;
    let live_workers = registry.all_workers();
    let live_ids: std::collections::HashSet<String> = live_workers
        .iter()
        .map(|w| w.info.worker_id.clone())
        .collect();

    // Build JSON for live workers (with heartbeat, resources, etc.)
    let mut data: Vec<Value> = live_workers
        .iter()
        .map(|w| {
            let last_hb = w.last_heartbeat.as_ref().map(|hb| {
                json!({
                    "used_cpu_percent": hb.used_cpu_percent,
                    "used_memory_mb": hb.used_memory_mb,
                    "used_disk_mb": hb.used_disk_mb,
                    "running_jobs": hb.running_job_ids.len(),
                    "system_load": hb.system_load,
                    "timestamp": hb.timestamp.to_rfc3339(),
                    "disk_details": hb.disk_details,
                })
            });
            let active_groups: Vec<Value> = jg_registry
                .get_groups_for_worker(&w.info.worker_id)
                .iter()
                .map(|g| {
                    let jobs = jg_registry.get_jobs_for_group(&g.id);
                    json!({
                        "group_id": g.id.to_string(),
                        "state": g.state.to_string(),
                        "branch": g.branch,
                        "commit_sha": g.commit_sha,
                        "allocated_cpu": g.allocated_resources.cpu,
                        "allocated_memory_mb": g.allocated_resources.memory_mb,
                        "allocated_disk_mb": g.allocated_resources.disk_mb,
                        "stages_submitted": jobs.len(),
                        "created_at": g.created_at.to_rfc3339(),
                    })
                })
                .collect();
            json!({
                "worker_id": w.info.worker_id,
                "hostname": w.info.hostname,
                "status": format!("{:?}", w.status),
                "total_cpu": w.info.total_cpu,
                "allocated_cpu": w.allocated_cpu,
                "available_cpu": w.available_cpu(),
                "total_memory_mb": w.info.total_memory_mb,
                "allocated_memory_mb": w.allocated_memory_mb,
                "available_memory_mb": w.available_memory_mb(),
                "total_disk_mb": w.info.total_disk_mb,
                "allocated_disk_mb": w.allocated_disk_mb,
                "available_disk_mb": w.available_disk_mb(),
                "disk_type": w.info.disk_type.to_string(),
                "docker_enabled": w.info.docker_enabled,
                "supported_job_types": w.info.supported_job_types,
                "registered_at": w.registered_at.to_rfc3339(),
                "last_heartbeat": last_hb,
                "disk_details": w.info.disk_details,
                "system_info": w.system_info,
                "active_groups": active_groups,
                "labels": w.info.labels,
                "priority": w.info.priority,
                "max_cpu": w.info.max_cpu,
                "max_memory_mb": w.info.max_memory_mb,
                "max_disk_mb": w.info.max_disk_mb,
                "max_cpu_percent": w.info.max_cpu_percent,
                "max_memory_percent": w.info.max_memory_percent,
                "max_disk_percent": w.info.max_disk_percent,
                "cpu_cap": w.cpu_cap(),
                "memory_cap": w.memory_cap(),
                "disk_cap": w.disk_cap(),
            })
        })
        .collect();
    drop(registry);
    drop(jg_registry);

    // Add registered-but-offline workers from DB
    if let Some(storage) = &state.storage {
        if let Ok(db_workers) = storage.list_workers().await {
            for row in db_workers {
                if !live_ids.contains(&row.worker_id) {
                    data.push(json!({
                        "worker_id": row.worker_id,
                        "hostname": row.hostname.unwrap_or_default(),
                        "status": "Offline",
                        "total_cpu": row.total_cpu.unwrap_or(0),
                        "allocated_cpu": 0,
                        "available_cpu": 0,
                        "total_memory_mb": row.total_memory_mb.unwrap_or(0),
                        "allocated_memory_mb": 0,
                        "available_memory_mb": 0,
                        "total_disk_mb": row.total_disk_mb.unwrap_or(0),
                        "allocated_disk_mb": 0,
                        "available_disk_mb": 0,
                        "disk_type": row.disk_type.as_deref().unwrap_or("unknown"),
                        "docker_enabled": row.docker_enabled,
                        "supported_job_types": row.supported_job_types,
                        "registered_at": row.registered_at.to_rfc3339(),
                        "last_heartbeat": row.last_heartbeat_at.map(|t| json!({ "timestamp": t.to_rfc3339() })),
                        "disk_details": [],
                        "system_info": row.system_info,
                        "active_groups": [],
                        "labels": row.labels,
                        "description": row.description,
                        "priority": row.priority,
                        "max_cpu": row.max_cpu,
                        "max_memory_mb": row.max_memory_mb,
                        "max_disk_mb": row.max_disk_mb,
                        "max_cpu_percent": row.max_cpu_percent,
                        "max_memory_percent": row.max_memory_percent,
                        "max_disk_percent": row.max_disk_percent,
                    }));
                }
            }
        }
    }

    let total = data.len() as i64;
    let paged: Vec<Value> = data
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(Json(json!({
        "data": paged,
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
            "disk_details": hb.disk_details,
        })
    });

    Ok(Json(json!({
        "worker_id": w.info.worker_id,
        "hostname": w.info.hostname,
        "status": format!("{:?}", w.status),
        "total_cpu": w.info.total_cpu,
        "allocated_cpu": w.allocated_cpu,
        "available_cpu": w.available_cpu(),
        "total_memory_mb": w.info.total_memory_mb,
        "allocated_memory_mb": w.allocated_memory_mb,
        "available_memory_mb": w.available_memory_mb(),
        "total_disk_mb": w.info.total_disk_mb,
        "allocated_disk_mb": w.allocated_disk_mb,
        "available_disk_mb": w.available_disk_mb(),
        "disk_type": w.info.disk_type.to_string(),
        "docker_enabled": w.info.docker_enabled,
        "supported_job_types": w.info.supported_job_types,
        "registered_at": w.registered_at.to_rfc3339(),
        "last_heartbeat": last_hb,
        "disk_details": w.info.disk_details,
        "system_info": w.system_info,
        "labels": w.info.labels,
        "priority": w.info.priority,
        "max_cpu": w.info.max_cpu,
        "max_memory_mb": w.info.max_memory_mb,
        "max_disk_mb": w.info.max_disk_mb,
        "max_cpu_percent": w.info.max_cpu_percent,
        "max_memory_percent": w.info.max_memory_percent,
        "max_disk_percent": w.info.max_disk_percent,
        "cpu_cap": w.cpu_cap(),
        "memory_cap": w.memory_cap(),
        "disk_cap": w.disk_cap(),
    })))
}

/// PUT /api/v1/workers/:id/metadata
/// Called by workers after gRPC registration to report OS/kernel/arch info.
/// No auth required (workers use gRPC tokens, not JWT).
pub async fn update_metadata(
    State(state): State<Arc<ControllerState>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let updated = {
        let mut registry = state.worker_registry.write().await;
        registry.update_system_info(&id, body.clone())
    };
    if !updated {
        return Err(ApiError::NotFound(format!("Worker '{}' not found", id)));
    }
    if let Some(storage) = &state.storage {
        if let Err(e) = storage.update_worker_metadata(&id, &body).await {
            tracing::warn!("Failed to persist metadata for worker {id}: {e}");
        }
    }
    Ok(Json(json!({ "status": "updated" })))
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

/// GET /api/v1/workers/:id/labels
pub async fn get_labels(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let registry = state.worker_registry.read().await;
    let labels = registry
        .get_labels(&id)
        .ok_or_else(|| ApiError::NotFound(format!("Worker '{}' not found", id)))?
        .to_vec();
    Ok(Json(json!({ "worker_id": id, "labels": labels })))
}

#[derive(Deserialize, Serialize)]
pub struct UpdateLabelsRequest {
    pub labels: Vec<String>,
}

/// PUT /api/v1/workers/:id/labels
pub async fn update_labels(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateLabelsRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let updated = {
        let mut registry = state.worker_registry.write().await;
        registry.update_labels(&id, body.labels.clone())
    };
    if !updated {
        return Err(ApiError::NotFound(format!("Worker '{}' not found", id)));
    }
    if let Some(storage) = &state.storage {
        if let Err(e) = storage.update_worker_labels(&id, &body.labels).await {
            tracing::warn!("Failed to persist labels for worker {id}: {e}");
        }
    }
    Ok(Json(json!({ "worker_id": id, "labels": body.labels })))
}

/// PUT /api/v1/workers/:id/approve
pub async fn approve(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_approved(&state, auth_user, &id, true).await
}

/// PUT /api/v1/workers/:id/reject
pub async fn reject(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_approved(&state, auth_user, &id, false).await
}

async fn set_approved(
    state: &Arc<ControllerState>,
    auth_user: AuthUser,
    id: &str,
    approved: bool,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    storage
        .update_worker_approved(id, approved)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let status = if approved { "approved" } else { "rejected" };
    info!("Worker {} {} via REST API", id, status);
    Ok(Json(json!({ "worker_id": id, "approved": approved })))
}

/// POST /api/v1/workers/register
#[derive(Deserialize)]
pub struct RegisterWorkerRequest {
    pub worker_id: String,
    pub hostname: String,
    pub labels: Option<Vec<String>>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub max_cpu: Option<i32>,
    pub max_memory_mb: Option<i64>,
    pub max_disk_mb: Option<i64>,
    pub max_cpu_percent: Option<i32>,
    pub max_memory_percent: Option<i32>,
    pub max_disk_percent: Option<i32>,
}

pub async fn register_worker(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<RegisterWorkerRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if body.worker_id.trim().is_empty() {
        return Err(ApiError::BadRequest("worker_id is required".into()));
    }
    if body.hostname.trim().is_empty() {
        return Err(ApiError::BadRequest("hostname is required".into()));
    }

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Check if worker already registered
    if let Ok(Some(_)) = storage.get_worker(&body.worker_id).await {
        return Err(ApiError::Conflict(format!(
            "Worker '{}' already registered. Use Regenerate Token to issue a new token.",
            body.worker_id
        )));
    }

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = format!(
        "chola_wkr_{}",
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    );
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let token_name = format!("worker-{}", body.worker_id);
    let labels = body.labels.unwrap_or_default();

    storage
        .register_worker(
            &body.worker_id,
            &body.hostname,
            &labels,
            body.description.as_deref(),
            &token_name,
            &token_hash,
            &auth_user.username,
            body.priority,
            body.max_cpu,
            body.max_memory_mb,
            body.max_disk_mb,
            body.max_cpu_percent,
            body.max_memory_percent,
            body.max_disk_percent,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Refresh in-memory token hash cache
    if let Ok(tokens) = storage.list_worker_tokens().await {
        let hashes: std::collections::HashSet<String> = tokens
            .into_iter()
            .filter(|t| t.active)
            .map(|t| t.token_hash.clone())
            .collect();
        if let Ok(mut guard) = state.token_hashes.write() {
            *guard = hashes;
        }
    }

    info!(
        "Worker {} registered by {}",
        body.worker_id, auth_user.username
    );
    Ok(Json(json!({
        "worker_id": body.worker_id,
        "token": token,
    })))
}

/// POST /api/v1/workers/:id/regenerate-token
pub async fn regenerate_token(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(worker_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // 1. Deactivate all existing active tokens for this worker
    storage
        .deactivate_tokens_for_worker(&worker_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 2. Generate new token
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = format!(
        "chola_wkr_{}",
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    );
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let token_name = format!("worker-{}-regen", worker_id);

    // 3. Insert new token row bound to this worker
    storage
        .create_worker_token(
            &token_name,
            &token_hash,
            "dedicated",
            Some(auth_user.username.as_str()),
            None,
            0,
            Some(worker_id.as_str()),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 4. Refresh in-memory token hash cache
    if let Ok(tokens) = storage.list_worker_tokens().await {
        let hashes: std::collections::HashSet<String> = tokens
            .into_iter()
            .filter(|t| t.active)
            .map(|t| t.token_hash.clone())
            .collect();
        if let Ok(mut guard) = state.token_hashes.write() {
            *guard = hashes;
        }
    }

    // 5. Kill existing job stream for this worker (forces reconnect with new token)
    {
        let mut senders = state.job_stream_senders.write().await;
        senders.remove(&worker_id);
    }

    info!(
        "Token regenerated for worker {} by {}",
        worker_id, auth_user.username
    );
    Ok(Json(json!({
        "worker_id": worker_id,
        "token": token,
    })))
}

/// DELETE /api/v1/workers/:id
pub async fn delete_worker(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Deactivate all tokens for this worker
    storage
        .deactivate_tokens_for_worker(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Refresh token hash cache
    if let Ok(tokens) = storage.list_worker_tokens().await {
        let hashes: std::collections::HashSet<String> = tokens
            .into_iter()
            .filter(|t| t.active)
            .map(|t| t.token_hash.clone())
            .collect();
        if let Ok(mut guard) = state.token_hashes.write() {
            *guard = hashes;
        }
    }

    // Disconnect job stream
    {
        let mut senders = state.job_stream_senders.write().await;
        senders.remove(&id);
    }

    // Remove from in-memory registry
    {
        let mut registry = state.worker_registry.write().await;
        registry.remove(&id);
    }

    // Delete from DB
    let deleted = storage
        .delete_worker(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !deleted {
        return Err(ApiError::NotFound(format!("Worker '{}' not found", id)));
    }

    info!("Worker {} deleted by {}", id, auth_user.username);
    Ok(Json(json!({ "worker_id": id, "deleted": true })))
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

/// Request body for PUT /api/v1/workers/:id/limits
#[derive(Deserialize)]
pub struct UpdateLimitsRequest {
    pub priority: Option<i32>,
    /// Max CPU cores chola may reserve. 0 or null = clear (use total_cpu).
    pub max_cpu: Option<i32>,
    /// Max memory MB chola may reserve. 0 or null = clear (use total_memory_mb).
    pub max_memory_mb: Option<i64>,
    /// Max disk MB chola may reserve. 0 or null = clear (use total_disk_mb).
    pub max_disk_mb: Option<i64>,
    /// Max CPU as percentage of total (1-100). 0 or null = clear.
    pub max_cpu_percent: Option<i32>,
    /// Max memory as percentage of total (1-100). 0 or null = clear.
    pub max_memory_percent: Option<i32>,
    /// Max disk as percentage of total (1-100). 0 or null = clear.
    pub max_disk_percent: Option<i32>,
}

/// PUT /api/v1/workers/:id/limits
pub async fn update_limits(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateLimitsRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_workers() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Persist to DB
    let row = storage
        .update_worker_priority_limits(
            &id,
            body.priority,
            body.max_cpu,
            body.max_memory_mb,
            body.max_disk_mb,
            body.max_cpu_percent,
            body.max_memory_percent,
            body.max_disk_percent,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Worker '{}' not found", id)))?;

    // Update in-memory registry if worker is live
    {
        let mut registry = state.worker_registry.write().await;
        registry.update_priority_limits(
            &id,
            body.priority,
            body.max_cpu
                .map(|v| if v == 0 { None } else { Some(v as u32) }),
            body.max_memory_mb
                .map(|v| if v == 0 { None } else { Some(v as u64) }),
            body.max_disk_mb
                .map(|v| if v == 0 { None } else { Some(v as u64) }),
            body.max_cpu_percent
                .map(|v| if v == 0 { None } else { Some(v) }),
            body.max_memory_percent
                .map(|v| if v == 0 { None } else { Some(v) }),
            body.max_disk_percent
                .map(|v| if v == 0 { None } else { Some(v) }),
        );
    }

    info!(
        "Worker {} limits updated by {}: priority={:?} max_cpu={:?} max_memory_mb={:?} max_disk_mb={:?} \
         max_cpu_percent={:?} max_memory_percent={:?} max_disk_percent={:?}",
        id, auth_user.username, body.priority, body.max_cpu, body.max_memory_mb, body.max_disk_mb,
        body.max_cpu_percent, body.max_memory_percent, body.max_disk_percent
    );

    Ok(Json(json!({
        "worker_id": row.worker_id,
        "priority": row.priority,
        "max_cpu": row.max_cpu,
        "max_memory_mb": row.max_memory_mb,
        "max_disk_mb": row.max_disk_mb,
        "max_cpu_percent": row.max_cpu_percent,
        "max_memory_percent": row.max_memory_percent,
        "max_disk_percent": row.max_disk_percent,
    })))
}
