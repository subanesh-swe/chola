use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    Json,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use tracing::{info, warn};
use uuid::Uuid;

use ci_core::models::stage::Webhook;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// ── HMAC verification ───────────────────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

fn verify_github_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let hex_sig = signature.strip_prefix("sha256=").unwrap_or(signature);
    let sig_bytes = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&sig_bytes).is_ok()
}

// ── Payload parsing ─────────────────────────────────────────────────────────

/// Extracted fields from a webhook push event.
struct PushPayload {
    branch: String,
    commit_sha: String,
}

fn parse_github_push(body: &Value) -> Option<PushPayload> {
    let git_ref = body.get("ref")?.as_str()?;
    let branch = git_ref.strip_prefix("refs/heads/")?;
    let after = body.get("after")?.as_str()?;
    // Skip zero-commit (branch deletion)
    if after == "0000000000000000000000000000000000000000" {
        return None;
    }
    Some(PushPayload {
        branch: branch.to_string(),
        commit_sha: after.to_string(),
    })
}

fn parse_gitlab_push(body: &Value) -> Option<PushPayload> {
    let git_ref = body.get("ref")?.as_str()?;
    let branch = git_ref.strip_prefix("refs/heads/")?;
    let after = body.get("after")?.as_str()?;
    if after == "0000000000000000000000000000000000000000" {
        return None;
    }
    Some(PushPayload {
        branch: branch.to_string(),
        commit_sha: after.to_string(),
    })
}

// ── Build trigger (reuses controller state) ─────────────────────────────────

async fn trigger_build(
    state: &Arc<ControllerState>,
    repo_id: Uuid,
    branch: &str,
    commit_sha: &str,
    trigger_source: &str,
) -> Result<Value, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Dedup: return existing active group
    if let Ok(Some(existing)) = storage
        .find_active_job_group(repo_id, Some(branch), Some(commit_sha))
        .await
    {
        return Ok(json!({
            "job_group_id": existing.id.to_string(),
            "state": existing.state.to_string(),
            "message": "Existing active group returned",
        }));
    }

    // Load stage configs for the repo
    let stages = storage
        .get_stage_configs_for_repo(repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if stages.is_empty() {
        return Err(ApiError::BadRequest(
            "Repo has no stage configs, cannot trigger build".into(),
        ));
    }

    // Create a new job group
    let mut group = ci_core::models::job_group::JobGroup::new(
        repo_id,
        Some(branch.to_string()),
        Some(commit_sha.to_string()),
    );
    group.trigger_source = trigger_source.to_string();

    // Try to reserve a worker
    let worker_id = pick_worker(state).await?;
    group.reserved_worker_id = Some(worker_id.clone());
    group.state = ci_core::models::job_group::JobGroupState::Reserved;
    group.updated_at = chrono::Utc::now();

    // Acquire Redis lock if available
    if let Some(redis) = &state.redis_store {
        let ok = redis
            .reserve_worker(
                &worker_id,
                &group.id.to_string(),
                state.config.workers.reservation_timeout_secs,
            )
            .await
            .unwrap_or(false);
        if !ok {
            return Err(ApiError::Conflict(
                "Could not acquire worker reservation".into(),
            ));
        }
        let _ = redis.remove_available_worker(&worker_id).await;
    }

    // Persist
    storage
        .create_job_group(&group)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let group_id = group.id;

    // Add to in-memory registry
    {
        let mut jg = state.job_group_registry.write().await;
        jg.add_group(group);
    }

    // Submit all stages as jobs
    submit_stages(state, group_id, &worker_id, &stages).await?;

    state.metrics.inc_worker_reservations();
    state.metrics.inc_active_builds();

    Ok(json!({
        "job_group_id": group_id.to_string(),
        "worker_id": worker_id,
        "stages_submitted": stages.len(),
        "message": "Build triggered",
    }))
}

async fn pick_worker(state: &Arc<ControllerState>) -> Result<String, ApiError> {
    let registry = state.worker_registry.read().await;
    let connected = registry.connected_workers();
    if connected.is_empty() {
        return Err(ApiError::Conflict("No connected workers available".into()));
    }
    Ok(connected[0].info.worker_id.clone())
}

async fn submit_stages(
    state: &Arc<ControllerState>,
    group_id: Uuid,
    worker_id: &str,
    stages: &[ci_core::models::stage::StageConfig],
) -> Result<(), ApiError> {
    use ci_core::models::job::{Job, JobType};

    for stage in stages {
        let job_id = format!("{}-{}", group_id, stage.stage_name);
        let mut job = Job::new(
            job_id.clone(),
            stage.command.clone(),
            JobType::Common,
            stage.required_cpu as u32,
            stage.required_memory_mb as u64,
            stage.required_disk_mb as u64,
        );
        job.job_group_id = Some(group_id);
        job.stage_name = Some(stage.stage_name.clone());
        job.assigned_worker = Some(worker_id.to_string());
        job.state = ci_core::models::job::JobState::Queued;
        job.max_duration_secs = Some(stage.max_duration_secs);

        // Add to registries
        {
            let mut jg = state.job_group_registry.write().await;
            if let Some(g) = jg.get(&group_id) {
                if g.state == ci_core::models::job_group::JobGroupState::Reserved {
                    jg.update_state(
                        &group_id,
                        ci_core::models::job_group::JobGroupState::Running,
                    );
                }
            }
            jg.add_job_to_group(&group_id, job.clone());
        }
        {
            let mut jr = state.job_registry.write().await;
            jr.add_job(job);
        }

        // Persist to DB
        if let Some(storage) = &state.storage {
            let now = chrono::Utc::now();
            let db_job = crate::storage::DbJob {
                id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, job_id.as_bytes()),
                job_group_id: group_id,
                stage_config_id: stage.id,
                stage_name: stage.stage_name.clone(),
                command: stage.command.clone(),
                pre_script: None,
                post_script: None,
                worker_id: Some(worker_id.to_string()),
                state: "queued".to_string(),
                exit_code: None,
                pre_exit_code: None,
                post_exit_code: None,
                log_path: None,
                started_at: None,
                completed_at: None,
                created_at: now,
                updated_at: now,
            };
            if let Err(e) = storage.create_job(&db_job).await {
                warn!("Failed to persist job {}: {}", job_id, e);
            }
        }

        state.metrics.inc_stages_submitted();
        state.metrics.inc_active_stages();
    }

    // Wake scheduler
    state.scheduler_notify.notify_waiters();
    Ok(())
}

// ── Webhook receive endpoint (public, no auth) ─────────────────────────────

/// POST /api/v1/webhooks/{provider}/{secret}
pub async fn receive(
    State(state): State<Arc<ControllerState>>,
    Path((provider, secret)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Look up webhook by secret
    let webhook = storage
        .get_webhook_by_secret(&secret)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Webhook not found".into()))?;

    if webhook.provider != provider {
        return Err(ApiError::BadRequest("Provider mismatch".into()));
    }

    // Verify signature based on provider
    match provider.as_str() {
        "github" => {
            let sig = headers
                .get("x-hub-signature-256")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !sig.is_empty() && !verify_github_signature(&webhook.secret, &body, sig) {
                return Err(ApiError::Unauthorized("Invalid signature".into()));
            }
        }
        "gitlab" => {
            let token = headers
                .get("x-gitlab-token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !token.is_empty() && token != webhook.secret {
                return Err(ApiError::Unauthorized("Invalid token".into()));
            }
        }
        _ => return Err(ApiError::BadRequest("Unsupported provider".into())),
    }

    // Parse payload
    let payload: Value =
        serde_json::from_slice(&body).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let push = match provider.as_str() {
        "github" => parse_github_push(&payload),
        "gitlab" => parse_gitlab_push(&payload),
        _ => None,
    };

    let push = match push {
        Some(p) => p,
        None => {
            return Ok(Json(
                json!({"message": "Event ignored (not a push or branch deleted)"}),
            ))
        }
    };

    // Check that the event type is in the webhook's events list
    let event_type = match provider.as_str() {
        "github" => headers
            .get("x-github-event")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("push"),
        "gitlab" => payload
            .get("object_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("push"),
        _ => "push",
    };

    if !webhook.events.iter().any(|e| e == event_type) {
        return Ok(Json(json!({
            "message": format!("Event '{}' not configured for this webhook", event_type),
        })));
    }

    info!(
        "Webhook trigger: provider={} repo_id={} branch={} commit={}",
        provider, webhook.repo_id, push.branch, push.commit_sha
    );

    let trigger_source = format!("webhook:{}", provider);
    let result = trigger_build(
        &state,
        webhook.repo_id,
        &push.branch,
        &push.commit_sha,
        &trigger_source,
    )
    .await?;

    Ok(Json(result))
}

// ── Webhook CRUD (protected, admin) ─────────────────────────────────────────

fn webhook_to_json(w: &Webhook) -> Value {
    json!({
        "id": w.id.to_string(),
        "repo_id": w.repo_id.to_string(),
        "provider": w.provider,
        "secret": w.secret,
        "events": w.events,
        "enabled": w.enabled,
        "created_at": w.created_at.to_rfc3339(),
        "updated_at": w.updated_at.to_rfc3339(),
    })
}

#[derive(Deserialize)]
pub struct CreateWebhookRequest {
    pub provider: String,
    pub events: Option<Vec<String>>,
}

/// GET /api/v1/repos/{id}/webhooks
pub async fn list_for_repo(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let hooks = storage
        .list_webhooks_for_repo(repo_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let list: Vec<Value> = hooks.iter().map(webhook_to_json).collect();
    Ok(Json(json!({ "webhooks": list, "count": list.len() })))
}

/// POST /api/v1/repos/{id}/webhooks
pub async fn create(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<CreateWebhookRequest>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    if !matches!(body.provider.as_str(), "github" | "gitlab") {
        return Err(ApiError::BadRequest(
            "provider must be 'github' or 'gitlab'".into(),
        ));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;

    // Generate a random secret
    let secret = format!("whsec_{}", Uuid::new_v4().simple());
    let events = body.events.unwrap_or_else(|| vec!["push".to_string()]);

    let webhook = storage
        .create_webhook(repo_id, &body.provider, &secret, &events)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(webhook_to_json(&webhook)))
}

/// DELETE /api/v1/repos/{repo_id}/webhooks/{webhook_id}
pub async fn delete(
    State(state): State<Arc<ControllerState>>,
    auth_user: AuthUser,
    Path((_repo_id, webhook_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    if !auth_user.role.can_manage_repos() {
        return Err(ApiError::Forbidden("Insufficient permissions".into()));
    }
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let deleted = storage
        .delete_webhook(webhook_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if deleted {
        Ok(Json(json!({"deleted": true})))
    } else {
        Err(ApiError::NotFound("Webhook not found".into()))
    }
}
