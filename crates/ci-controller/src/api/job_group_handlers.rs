use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;
use uuid::Uuid;

use ci_core::models::job_group::JobGroupState;
use ci_core::proto::orchestrator::ReserveWorkerRequest;

use crate::auth::middleware::AuthUser;
use crate::grpc_server::do_reserve_worker;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Query params ─────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub state: Option<String>,
    pub repo_id: Option<Uuid>,
}

// ── Request bodies ───────────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TriggerRequest {
    pub repo_name: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub stages: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub idempotency_key: Option<String>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/v1/job-groups
#[utoipa::path(
    get,
    path = "/api/v1/job-groups",
    tag = "Builds",
    params(
        ("limit" = Option<i64>, Query, description = "Page size"),
        ("offset" = Option<i64>, Query, description = "Offset"),
        ("state" = Option<String>, Query, description = "Filter by state"),
        ("repo_id" = Option<uuid::Uuid>, Query, description = "Filter by repo"),
    ),
    responses(
        (status = 200, description = "Paginated job group list"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn list(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (groups, total) = storage
        .list_job_groups_paginated(limit, offset, params.state.as_deref(), params.repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Build repo_id -> repo_name lookup
    let repo_ids: Vec<Uuid> = groups
        .iter()
        .filter_map(|g| g.repo_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut repo_names: HashMap<Uuid, String> = HashMap::new();
    for rid in &repo_ids {
        if let Ok(Some(repo)) = storage.get_repo(*rid).await {
            repo_names.insert(*rid, repo.repo_name);
        }
    }

    let list: Vec<Value> = groups
        .iter()
        .map(|g| {
            json!({
                "id": g.id.to_string(),
                "repo_id": g.repo_id.map(|r| r.to_string()),
                "repo_name": g.repo_id.and_then(|r| repo_names.get(&r).cloned()).unwrap_or_default(),
                "branch": g.branch,
                "commit_sha": g.commit_sha,
                "trigger_source": g.trigger_source,
                "reserved_worker_id": g.reserved_worker_id,
                "state": g.state.to_string(),
                "status_reason": g.status_reason,
                "created_at": g.created_at.to_rfc3339(),
                "updated_at": g.updated_at.to_rfc3339(),
                "completed_at": g.completed_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": list,
        "pagination": { "total": total, "limit": limit, "offset": offset },
    })))
}

/// GET /api/v1/job-groups/:id
pub async fn get_one(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let (group, jobs) = storage
        .get_job_group_with_jobs(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Job group not found".into()))?;

    // Look up repo name
    let repo_name = if let Some(rid) = group.repo_id {
        storage
            .get_repo(rid)
            .await
            .ok()
            .flatten()
            .map(|r| r.repo_name)
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Look up reserved stages from stage_configs for this repo
    let reserved_stages: Vec<String> = if let Some(rid) = group.repo_id {
        storage
            .get_stage_configs_for_repo(rid)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|sc| sc.stage_name)
            .collect()
    } else {
        Vec::new()
    };

    // Look up allocated resources from in-memory registry
    let alloc = {
        let jg = state.job_group_registry.read().await;
        jg.get(&id)
            .map(|g| g.allocated_resources)
            .unwrap_or_default()
    };

    // If no in-memory allocation, compute from stage_configs as fallback
    let alloc_json = if alloc.cpu > 0 || alloc.memory_mb > 0 || alloc.disk_mb > 0 {
        json!({
            "cpu": alloc.cpu,
            "memory_mb": alloc.memory_mb,
            "disk_mb": alloc.disk_mb,
        })
    } else if let Some(rid) = group.repo_id {
        let stages = storage
            .get_stage_configs_for_repo(rid)
            .await
            .unwrap_or_default();
        if stages.is_empty() {
            json!({ "cpu": 0, "memory_mb": 0, "disk_mb": 0 })
        } else {
            let max_cpu = stages
                .iter()
                .map(|s| s.required_cpu.max(0))
                .max()
                .unwrap_or(0);
            let max_mem = stages
                .iter()
                .map(|s| s.required_memory_mb.max(0))
                .max()
                .unwrap_or(0);
            let max_disk = stages
                .iter()
                .map(|s| s.required_disk_mb.max(0))
                .max()
                .unwrap_or(0);
            json!({ "cpu": max_cpu, "memory_mb": max_mem, "disk_mb": max_disk })
        }
    } else {
        json!({ "cpu": 0, "memory_mb": 0, "disk_mb": 0 })
    };

    // Look up max_duration_secs per stage from stage_configs
    let stage_timeouts: std::collections::HashMap<String, i32> = if let Some(rid) = group.repo_id {
        storage
            .get_stage_configs_for_repo(rid)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|sc| (sc.stage_name, sc.max_duration_secs))
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    let job_list: Vec<Value> = jobs
        .iter()
        .map(|j| {
            let max_dur = stage_timeouts.get(&j.stage_name).copied().unwrap_or(0);
            json!({
                "id": j.id.to_string(),
                "stage_name": j.stage_name,
                "command": j.command,
                "worker_id": j.worker_id,
                "state": j.state,
                "exit_code": j.exit_code,
                "pre_exit_code": j.pre_exit_code,
                "post_exit_code": j.post_exit_code,
                "status_reason": j.status_reason,
                "max_duration_secs": max_dur,
                "started_at": j.started_at.map(|t| t.to_rfc3339()),
                "completed_at": j.completed_at.map(|t| t.to_rfc3339()),
                "created_at": j.created_at.to_rfc3339(),
                "updated_at": j.updated_at.to_rfc3339(),
            })
        })
        .collect();

    // Compute timeout info from in-memory state (respect DB overrides)
    let idle_cfg = state
        .resolve_setting_u64(
            "workers.idle_timeout_secs",
            state.config.workers.idle_timeout_secs,
        )
        .await;
    let stall_cfg = state
        .resolve_setting_u64(
            "workers.stall_timeout_secs",
            state.config.workers.stall_timeout_secs,
        )
        .await;

    let (last_activity_at, time_until_timeout) = {
        let jg = state.job_group_registry.read().await;
        if let Some(g) = jg.get(&id) {
            let idle_secs = (chrono::Utc::now() - g.last_activity_at)
                .num_seconds()
                .max(0) as u64;
            let timeout = match g.state {
                ci_core::models::job_group::JobGroupState::Reserved => idle_cfg,
                ci_core::models::job_group::JobGroupState::Running => {
                    // No group timeout while a stage is actively running
                    let has_running = jg.get_jobs_for_group(&id).iter().any(|j| {
                        matches!(
                            j.state,
                            ci_core::models::job::JobState::Running
                                | ci_core::models::job::JobState::Assigned
                        )
                    });
                    if has_running {
                        0
                    } else {
                        stall_cfg
                    }
                }
                _ => 0,
            };
            let remaining = if timeout > 0 {
                timeout.saturating_sub(idle_secs) as i64
            } else {
                -1i64
            };
            (Some(g.last_activity_at.to_rfc3339()), remaining)
        } else {
            (None, -1i64)
        }
    };

    // Reservation TTL from Redis
    let reservation_ttl =
        if let (Some(redis), Some(ref wid)) = (&state.redis_store, &group.reserved_worker_id) {
            redis
                .get_reservation_ttl(wid, &id.to_string())
                .await
                .unwrap_or(None)
        } else {
            None
        };

    Ok(Json(json!({
        "id": group.id.to_string(),
        "repo_id": group.repo_id.map(|r| r.to_string()),
        "repo_name": repo_name,
        "branch": group.branch,
        "commit_sha": group.commit_sha,
        "trigger_source": group.trigger_source,
        "reserved_worker_id": group.reserved_worker_id,
        "state": group.state.to_string(),
        "status_reason": group.status_reason,
        "reserved_stages": reserved_stages,
        "allocated_resources": alloc_json,
        "created_at": group.created_at.to_rfc3339(),
        "updated_at": group.updated_at.to_rfc3339(),
        "completed_at": group.completed_at.map(|t| t.to_rfc3339()),
        "jobs": job_list,
        "last_activity_at": last_activity_at,
        "time_until_timeout_secs": time_until_timeout,
        "reservation_expires_in_secs": reservation_ttl,
        "idle_timeout_secs": idle_cfg,
        "stall_timeout_secs": stall_cfg,
    })))
}

/// POST /api/v1/job-groups/:id/cancel
pub async fn cancel(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_cancel_jobs() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    // Update in-memory registry, extract worker/resources for release
    let api_reason = format!("Cancelled via API by {}", auth_user.username);
    let release_info = {
        let mut jg = state.job_group_registry.write().await;
        if let Some(group) = jg.get(&id) {
            if group.state.is_terminal() {
                return Err(ApiError::Conflict(
                    "Job group already in terminal state".into(),
                ));
            }
        }
        let info = jg.get(&id).map(|g| {
            (
                g.reserved_worker_id.clone(),
                g.allocated_resources,
                g.repo_id,
                g.branch.clone(),
                g.commit_sha.clone(),
            )
        });
        jg.update_state(&id, JobGroupState::Cancelled);
        if let Some(g) = jg.get_mut(&id) {
            g.status_reason = Some(api_reason.clone());
        }
        jg.fail_group_jobs(&id, &api_reason);
        info
    };

    // Dispatch global post-script before releasing worker resources
    if let Some((Some(ref wid), _, repo_id, ref branch, ref commit_sha)) = release_info {
        crate::grpc_server::dispatch_global_post_script(
            &state,
            &id,
            wid,
            repo_id,
            branch.clone(),
            commit_sha.clone(),
            JobGroupState::Cancelled,
        )
        .await;
    }

    // Release allocated resources on the worker
    if let Some((Some(ref wid), alloc, _, _, _)) = release_info {
        if alloc.cpu > 0 || alloc.memory_mb > 0 || alloc.disk_mb > 0 {
            let mut wr = state.worker_registry.write().await;
            if let Some(w) = wr.get_mut(wid) {
                w.release(alloc.cpu, alloc.memory_mb, alloc.disk_mb);
            }
        }
    }

    // Release Redis reservation so the worker can accept new groups
    if let Some((Some(ref wid), _, _, _, _)) = release_info {
        if let Some(redis) = &state.redis_store {
            if let Err(e) = crate::reservation::ReservationManager::release(redis, wid, &id).await {
                warn!("Failed to release Redis reservation for worker {wid}: {e}");
            }
        }
    }

    // Update in DB
    if let Some(storage) = &state.storage {
        if let Err(e) = storage
            .update_job_group_state(id, JobGroupState::Cancelled, Some(&api_reason))
            .await
        {
            warn!("Failed to persist cancel state for group {id}: {e}");
        }
        // Persist job cancellations so they survive restarts
        if let Err(e) = storage.cancel_jobs_for_group(id).await {
            warn!("Failed to cancel orphaned jobs in DB for group {id}: {e}");
        }
    }

    if let Some(storage) = &state.storage {
        super::audit::audit_action(
            storage,
            auth_user.user_id,
            &auth_user.username,
            "cancel_job_group",
            "job_group",
            &id.to_string(),
        )
        .await;
    }

    Ok(Json(json!({"id": id.to_string(), "state": "cancelled"})))
}

/// POST /api/v1/job-groups  — trigger a new build from REST
#[utoipa::path(
    post,
    path = "/api/v1/job-groups",
    tag = "Builds",
    request_body = TriggerRequest,
    responses(
        (status = 200, description = "Build triggered"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Insufficient permissions"),
        (status = 409, description = "Conflict"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn trigger(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Json(body): Json<TriggerRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_trigger_builds() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    let req = ReserveWorkerRequest {
        repo_name: body.repo_name.clone(),
        repo_url: String::new(),
        branch: body.branch.unwrap_or_default(),
        commit_sha: body.commit_sha.unwrap_or_default(),
        stages: body.stages.unwrap_or_default(),
        priority: body.priority.unwrap_or(0),
        idempotency_key: body.idempotency_key.clone().unwrap_or_default(),
    };

    let resp = do_reserve_worker(&state, &req)
        .await
        .map_err(|e| ApiError::Internal(e.message().to_string()))?;

    if !resp.success {
        return Err(ApiError::Conflict(resp.message));
    }

    let stages: Vec<Value> = resp
        .stages
        .iter()
        .map(|s| json!({"stage_name": s.stage_name}))
        .collect();

    Ok(Json(json!({
        "job_group_id": resp.job_group_id,
        "worker_id": resp.worker_id,
        "stages": stages,
        "message": resp.message,
    })))
}

/// POST /api/v1/job-groups/:id/retry  — re-run a failed/cancelled build
pub async fn retry(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_trigger_builds() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }

    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Look up original group
    let (original, original_jobs) = storage
        .get_job_group_with_jobs(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Job group not found".into()))?;

    if !original.state.is_terminal() {
        return Err(ApiError::Conflict(
            "Can only retry terminal (failed/cancelled/success) builds".into(),
        ));
    }

    // Look up repo name from repo_id
    let rid = original
        .repo_id
        .ok_or_else(|| ApiError::BadRequest("Cannot retry ad-hoc builds without a repo".into()))?;
    let repo = storage
        .get_repo(rid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Repo no longer exists".into()))?;

    // Collect stage names from original jobs
    let stages: Vec<String> = original_jobs.iter().map(|j| j.stage_name.clone()).collect();

    let req = ReserveWorkerRequest {
        repo_name: repo.repo_name,
        repo_url: repo.repo_url,
        branch: original.branch.unwrap_or_default(),
        commit_sha: original.commit_sha.unwrap_or_default(),
        stages,
        priority: 0,
        idempotency_key: String::new(),
    };

    let resp = do_reserve_worker(&state, &req)
        .await
        .map_err(|e| ApiError::Internal(e.message().to_string()))?;

    if !resp.success {
        warn!("Retry of group {} failed: {}", id, resp.message);
        return Err(ApiError::Conflict(resp.message));
    }

    Ok(Json(json!({
        "job_group_id": resp.job_group_id,
        "worker_id": resp.worker_id,
        "original_group_id": id.to_string(),
        "message": "Build re-triggered successfully",
    })))
}

/// GET /api/v1/job-groups/:id/stages — stages with config + execution status + reservation info
pub async fn stages(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    let (group, jobs) = storage
        .get_job_group_with_jobs(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Job group not found".into()))?;

    // Repo name
    let repo_name = if let Some(rid) = group.repo_id {
        storage
            .get_repo(rid)
            .await
            .ok()
            .flatten()
            .map(|r| r.repo_name)
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Stage configs for this repo
    let stage_configs = if let Some(rid) = group.repo_id {
        storage
            .get_stage_configs_for_repo(rid)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Allocated resources from in-memory registry
    let alloc = {
        let jg = state.job_group_registry.read().await;
        jg.get(&id)
            .map(|g| g.allocated_resources)
            .unwrap_or_default()
    };

    // Reservation TTL from Redis (per-group key)
    let reservation_ttl =
        if let (Some(redis), Some(ref wid)) = (&state.redis_store, &group.reserved_worker_id) {
            redis
                .get_reservation_ttl(wid, &id.to_string())
                .await
                .unwrap_or(None)
        } else {
            None
        };

    // Compute time until inactivity timeout using last_activity_at (respect DB overrides)
    let idle_cfg = state
        .resolve_setting_u64(
            "workers.idle_timeout_secs",
            state.config.workers.idle_timeout_secs,
        )
        .await;
    let stall_cfg = state
        .resolve_setting_u64(
            "workers.stall_timeout_secs",
            state.config.workers.stall_timeout_secs,
        )
        .await;
    let time_until_timeout = {
        let jg = state.job_group_registry.read().await;
        jg.get(&id)
            .map(|g| {
                let idle_secs = (chrono::Utc::now() - g.last_activity_at)
                    .num_seconds()
                    .max(0) as u64;
                let timeout: u64 = match g.state {
                    ci_core::models::job_group::JobGroupState::Reserved => idle_cfg,
                    ci_core::models::job_group::JobGroupState::Running => {
                        let has_running = jg.get_jobs_for_group(&id).iter().any(|j| {
                            matches!(
                                j.state,
                                ci_core::models::job::JobState::Running
                                    | ci_core::models::job::JobState::Assigned
                            )
                        });
                        if has_running {
                            0
                        } else {
                            stall_cfg
                        }
                    }
                    _ => 0,
                };
                if timeout > 0 {
                    timeout.saturating_sub(idle_secs) as i64
                } else {
                    -1i64
                }
            })
            .unwrap_or(-1)
    };

    // Index jobs by stage_name for O(1) lookup
    let jobs_by_stage: HashMap<String, &crate::storage::DbJob> = jobs
        .iter()
        .filter_map(|j| Some((j.stage_name.clone(), j)))
        .collect();

    // Merge stage configs with job execution status
    let stage_list: Vec<Value> = stage_configs
        .iter()
        .map(|sc| {
            let job = jobs_by_stage.get(&sc.stage_name);
            let mut stage = json!({
                "stage_name": sc.stage_name,
                "command": sc.command,
                "command_mode": sc.command_mode,
                "required_cpu": sc.required_cpu,
                "required_memory_mb": sc.required_memory_mb,
                "required_disk_mb": sc.required_disk_mb,
                "max_duration_secs": sc.max_duration_secs,
                "execution_order": sc.execution_order,
                "parallel_group": sc.parallel_group,
                "depends_on": sc.depends_on,
                "job_type": sc.job_type,
                "required_labels": sc.required_labels,
                "max_retries": sc.max_retries,
            });
            if let Some(j) = job {
                stage["status"] = json!(j.state);
                stage["job_id"] = json!(j.id.to_string());
                stage["exit_code"] = json!(j.exit_code);
                stage["pre_exit_code"] = json!(j.pre_exit_code);
                stage["post_exit_code"] = json!(j.post_exit_code);
                stage["started_at"] = json!(j.started_at.map(|t| t.to_rfc3339()));
                stage["completed_at"] = json!(j.completed_at.map(|t| t.to_rfc3339()));
            } else {
                stage["status"] = json!("pending");
                stage["job_id"] = json!(null);
            }
            stage
        })
        .collect();

    Ok(Json(json!({
        "job_group_id": id.to_string(),
        "repo_name": repo_name,
        "branch": group.branch,
        "commit_sha": group.commit_sha,
        "worker_id": group.reserved_worker_id,
        "state": group.state,
        "reservation_expires_in_secs": reservation_ttl,
        "last_activity_at": group.last_activity_at.to_rfc3339(),
        "time_until_timeout_secs": time_until_timeout,
        "idle_timeout_secs": idle_cfg,
        "stall_timeout_secs": stall_cfg,
        "allocated_resources": {
            "cpu": alloc.cpu,
            "memory_mb": alloc.memory_mb,
            "disk_mb": alloc.disk_mb,
        },
        "stages": stage_list,
    })))
}
