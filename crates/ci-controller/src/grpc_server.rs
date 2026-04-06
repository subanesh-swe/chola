use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tonic::service::interceptor::InterceptedService;
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use ci_core::models::job::{Job, JobType};
use ci_core::proto::orchestrator::{
    orchestrator_server::{Orchestrator, OrchestratorServer},
    CancelDirective, CancelJobRequest, CancelJobResponse, GetJobGroupStatusRequest,
    GetJobGroupStatusResponse, GetJobStatusRequest, GetJobStatusResponse, HeartbeatAck,
    HeartbeatMessage, JobAssignment, JobStatusAck, JobStatusUpdate, JobStreamRequest, LogAck,
    LogChunk, LogResumeDirective, ReconnectRequest, ReconnectResponse, RegisterRequest,
    RegisterResponse, ReserveWorkerRequest, ReserveWorkerResponse, SubmitJobRequest,
    SubmitJobResponse, SubmitStageRequest, SubmitStageResponse, WatchJobLogsRequest,
};

use crate::dag;
use crate::reservation::ReservationManager;
use crate::scheduler::{BestFitScheduler, Scheduler};
use crate::state::ControllerState;

// ---------------------------------------------------------------------------
// Auth interceptor
// ---------------------------------------------------------------------------

fn auth_interceptor(
    config: ci_core::models::config::AuthConfig,
) -> impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone {
    move |req: tonic::Request<()>| -> Result<tonic::Request<()>, tonic::Status> {
        if !config.enabled {
            return Ok(req);
        }

        let token = config.token.as_deref().unwrap_or("");
        if token.is_empty() {
            return Ok(req);
        }

        let meta = req.metadata();
        let auth = meta
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let expected = format!("Bearer {}", token);
        if auth != expected {
            return Err(tonic::Status::unauthenticated(
                "Invalid or missing auth token",
            ));
        }

        Ok(req)
    }
}

// ---------------------------------------------------------------------------
// Helper functions (extracted from RPC handlers)
// ---------------------------------------------------------------------------

/// Build a `JobAssignment` proto from a domain `Job`.
fn build_job_assignment(job: Job) -> JobAssignment {
    JobAssignment {
        job_id: job.job_id,
        command: job.command,
        job_type: job.job_type.to_string(),
        required_cpu: job.required_cpu,
        required_memory_mb: job.required_memory_mb,
        required_disk_mb: job.required_disk_mb,
        isolation_required: job.isolation_required,
        branch_id: job.branch_id.unwrap_or_default(),
        environment: job.environment,
        cancel: None,
        job_group_id: job
            .job_group_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        stage_name: job.stage_name.unwrap_or_default(),
        pre_script: job.pre_script.unwrap_or_default(),
        post_script: job.post_script.unwrap_or_default(),
        max_duration_secs: job.max_duration_secs.unwrap_or(0),
        secret_env_keys: Vec::new(),
    }
}

/// Build a cancel-only `JobAssignment` for sending a cancellation directive to a worker.
fn build_cancel_assignment(job_id: &str, reason: &str) -> JobAssignment {
    JobAssignment {
        job_id: job_id.to_string(),
        command: String::new(),
        job_type: String::new(),
        required_cpu: 0,
        required_memory_mb: 0,
        required_disk_mb: 0,
        isolation_required: false,
        branch_id: String::new(),
        environment: std::collections::HashMap::new(),
        cancel: Some(CancelDirective {
            job_id: job_id.to_string(),
            reason: reason.to_string(),
            signal: 2,
        }),
        job_group_id: String::new(),
        stage_name: String::new(),
        pre_script: String::new(),
        post_script: String::new(),
        max_duration_secs: 0,
        secret_env_keys: Vec::new(),
    }
}

/// Convert a domain `JobState` to its protobuf i32 representation.
fn job_state_to_proto(state: ci_core::models::job::JobState) -> i32 {
    match state {
        ci_core::models::job::JobState::Queued => {
            ci_core::proto::orchestrator::JobState::Queued as i32
        }
        ci_core::models::job::JobState::Assigned => {
            ci_core::proto::orchestrator::JobState::Assigned as i32
        }
        ci_core::models::job::JobState::Running => {
            ci_core::proto::orchestrator::JobState::Running as i32
        }
        ci_core::models::job::JobState::Success => {
            ci_core::proto::orchestrator::JobState::Success as i32
        }
        ci_core::models::job::JobState::Failed => {
            ci_core::proto::orchestrator::JobState::Failed as i32
        }
        ci_core::models::job::JobState::Cancelled => {
            ci_core::proto::orchestrator::JobState::Cancelled as i32
        }
        ci_core::models::job::JobState::Unknown => {
            ci_core::proto::orchestrator::JobState::Unknown as i32
        }
    }
}

/// Try to dispatch a queued job to the given worker via its job stream channel.
///
/// Returns `true` if the channel is still open (caller should continue looping),
/// `false` if the channel closed (caller should break).
///
/// Lock ordering: worker_registry(R) -> job_registry(W) to match the heartbeat
/// timeout task and avoid ABBA deadlock.
async fn dispatch_job_for_worker(
    state: &Arc<ControllerState>,
    worker_id: &str,
    job_tx: &tokio::sync::mpsc::Sender<Result<JobAssignment, Status>>,
) -> bool {
    let scheduler = BestFitScheduler {
        nvme_preference: state.config.scheduling.nvme_preference,
        branch_affinity: true,
    };

    // Acquire in consistent order: worker_registry(R) first, then job_registry(W)
    let worker_registry = state.worker_registry.read().await;

    let worker_state = match worker_registry.get(worker_id) {
        Some(ws) => ws,
        None => return true,
    };

    let mut job_registry = state.job_registry.write().await;

    // Find a suitable job under the write lock — highest priority first
    let job_id_to_assign: Option<String> = {
        let queued = job_registry.queued_jobs();
        let mut sorted = queued;
        sorted.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        sorted.iter().find_map(|queued_job| {
            let targeted_elsewhere = queued_job
                .assigned_worker
                .as_deref()
                .map(|w| w != worker_id)
                .unwrap_or(false);
            if targeted_elsewhere {
                return None;
            }

            let explicitly_targeted = queued_job
                .assigned_worker
                .as_deref()
                .map(|w| w == worker_id)
                .unwrap_or(false);

            if explicitly_targeted
                || scheduler
                    .select_worker(queued_job, &[worker_state])
                    .is_some()
            {
                Some(queued_job.job_id.clone())
            } else {
                None
            }
        })
    };

    // Drop worker_registry before async send
    drop(worker_registry);

    // Assign the specific job the scheduler selected (not an arbitrary queued job)
    if let Some(job_id) = job_id_to_assign {
        if let Some(job) = job_registry.assign_job(&job_id, worker_id) {
            let assignment = build_job_assignment(job);
            drop(job_registry); // Release before async send
            if job_tx.send(Ok(assignment)).await.is_err() {
                return false;
            }
            return true;
        }
    }
    drop(job_registry);

    true
}

/// Compute resource requirements for a reservation from stage_configs.
///
/// Returns (max_cpu, sum_memory, max_disk). Uses max for CPU/disk (stages run
/// sequentially) and sum for memory (conservative). Falls back to (0,0,0)
/// when stages have no resource config, triggering whole-worker mode.
async fn compute_resource_needs(
    state: &Arc<ControllerState>,
    req: &ReserveWorkerRequest,
) -> (u32, u64, u64) {
    let stage_configs = if let Some(storage) = &state.storage {
        // Look up repo_id from repo_name
        let repo_id = match storage.get_repo_by_name(&req.repo_name).await {
            Ok(Some(repo)) => repo.id,
            _ => return (0, 0, 0),
        };
        storage
            .get_stage_configs_for_repo(repo_id)
            .await
            .unwrap_or_default()
    } else {
        return (0, 0, 0);
    };

    // Filter to requested stages if specified
    let relevant: Vec<_> = if req.stages.is_empty() {
        stage_configs.iter().collect()
    } else {
        stage_configs
            .iter()
            .filter(|s| req.stages.contains(&s.stage_name))
            .collect()
    };

    if relevant.is_empty() {
        return (0, 0, 0);
    }

    let needed_cpu = relevant
        .iter()
        .map(|s| s.required_cpu.max(0) as u32)
        .max()
        .unwrap_or(0);
    let needed_memory = relevant
        .iter()
        .map(|s| s.required_memory_mb.max(0) as u64)
        .max()
        .unwrap_or(0);
    let needed_disk = relevant
        .iter()
        .map(|s| s.required_disk_mb.max(0) as u64)
        .max()
        .unwrap_or(0);

    (needed_cpu, needed_memory, needed_disk)
}

/// Core logic for the `reserve_worker` RPC.
///
/// Resource-aware worker reservation.
///
/// Computes resource requirements from stage_configs, then selects a worker
/// with enough available capacity. Multiple builds can share a worker when
/// resources allow. Falls back to whole-worker reservation when stages have
/// no resource requirements configured.
pub async fn do_reserve_worker(
    state: &Arc<ControllerState>,
    req: &ReserveWorkerRequest,
) -> Result<ReserveWorkerResponse, Status> {
    // Generate group_id upfront so Redis lock references it
    let group_id = uuid::Uuid::new_v4();

    // Compute resource requirements from stage_configs
    let (needed_cpu, needed_memory, needed_disk) = compute_resource_needs(state, req).await;

    // Snapshot candidate worker IDs that have enough resources, then drop lock.
    let mut candidate_ids: Vec<String> = {
        let registry = state.worker_registry.read().await;
        let connected = registry.connected_workers();

        if connected.is_empty() {
            return Ok(ReserveWorkerResponse {
                job_group_id: String::new(),
                worker_id: String::new(),
                stages: Vec::new(),
                success: false,
                message: "No connected workers available".to_string(),
            });
        }

        // Filter by available resources (0 needed = no resource filter)
        let has_resource_req = needed_cpu > 0 || needed_memory > 0 || needed_disk > 0;
        connected
            .iter()
            .filter(|w| {
                if !has_resource_req {
                    return true;
                }
                w.available_cpu() >= needed_cpu
                    && w.available_memory_mb() >= needed_memory
                    && w.available_disk_mb() >= needed_disk
            })
            .map(|w| w.info.worker_id.clone())
            .collect()
    };
    // worker_registry lock dropped here

    if candidate_ids.is_empty() {
        return Ok(ReserveWorkerResponse {
            job_group_id: String::new(),
            worker_id: String::new(),
            stages: Vec::new(),
            success: false,
            message: "No workers with sufficient resources available".to_string(),
        });
    }

    // Allocate resources atomically via WorkerState::allocate() under write lock.
    // Sort by most available resources first (least-loaded scheduling).
    let has_resource_req = needed_cpu > 0 || needed_memory > 0 || needed_disk > 0;
    let worker_id = {
        let mut registry = state.worker_registry.write().await;
        // Sort candidates: most free resources first
        candidate_ids.sort_by(|a, b| {
            let free_a = registry
                .get(a)
                .map(|w| w.available_cpu() as u64 + w.available_memory_mb())
                .unwrap_or(0);
            let free_b = registry
                .get(b)
                .map(|w| w.available_cpu() as u64 + w.available_memory_mb())
                .unwrap_or(0);
            free_b.cmp(&free_a) // descending: most free first
        });
        let mut selected = None;
        for wid in &candidate_ids {
            if let Some(w) = registry.get_mut(wid) {
                if !has_resource_req || w.allocate(needed_cpu, needed_memory, needed_disk) {
                    selected = Some(wid.clone());
                    break;
                }
            }
        }
        selected
    };
    // write lock dropped

    let worker_id = match worker_id {
        Some(id) => id,
        None => {
            return Ok(ReserveWorkerResponse {
                job_group_id: String::new(),
                worker_id: String::new(),
                stages: Vec::new(),
                success: false,
                message: "No workers with sufficient resources available".to_string(),
            });
        }
    };

    // Record reservation in Redis as non-exclusive per-group tracking key
    if let Some(redis) = &state.redis_store {
        if let Err(e) = ReservationManager::reserve(
            redis,
            &worker_id,
            &group_id,
            state.config.workers.reservation_timeout_secs,
        )
        .await
        {
            warn!(
                "Failed to record Redis reservation for worker {}: {}",
                worker_id, e
            );
        }
    }

    // Check branch blacklist for selected worker
    if let Some(storage) = &state.storage {
        if !req.branch.is_empty() {
            let patterns = storage
                .get_active_branch_blacklist(&worker_id)
                .await
                .unwrap_or_default();
            for pattern in &patterns {
                if let Ok(re) = regex::Regex::new(pattern) {
                    if re.is_match(&req.branch) {
                        return Err(Status::failed_precondition(format!(
                            "Branch '{}' is blacklisted on worker '{}'",
                            req.branch, worker_id
                        )));
                    }
                }
            }
        }
    }

    // Look up repo by name — must exist in DB
    let (repo_id, max_concurrent, cancel_superseded) = if let Some(storage) = &state.storage {
        match storage.get_repo_by_name(&req.repo_name).await {
            Ok(Some(repo)) => (repo.id, repo.max_concurrent_builds, repo.cancel_superseded),
            Ok(None) => {
                return Ok(ReserveWorkerResponse {
                    job_group_id: String::new(),
                    worker_id: String::new(),
                    stages: Vec::new(),
                    success: false,
                    message: format!(
                        "Repo '{}' not found — create it in the DB first",
                        req.repo_name
                    ),
                });
            }
            Err(e) => {
                warn!("Failed to lookup repo {}: {}", req.repo_name, e);
                (uuid::Uuid::new_v4(), 0, false)
            }
        }
    } else {
        (uuid::Uuid::new_v4(), 0, false) // no DB — in-memory only
    };

    // Validate requested stages exist in DB
    if !req.stages.is_empty() {
        if let Some(storage) = &state.storage {
            let configs = storage
                .get_stage_configs_for_repo(repo_id)
                .await
                .unwrap_or_default();
            let known: std::collections::HashSet<&str> =
                configs.iter().map(|c| c.stage_name.as_str()).collect();
            let unknown: Vec<&str> = req
                .stages
                .iter()
                .filter(|s| !s.is_empty() && !known.contains(s.as_str()))
                .map(|s| s.as_str())
                .collect();
            if !unknown.is_empty() {
                return Err(Status::invalid_argument(format!(
                    "Unknown stages for repo '{}': {}",
                    req.repo_name,
                    unknown.join(", ")
                )));
            }
        }
    }

    // Enforce concurrency limits
    if max_concurrent > 0 {
        if let Some(storage) = &state.storage {
            let active = storage
                .count_active_groups_for_repo(repo_id)
                .await
                .unwrap_or(0);
            if active >= max_concurrent as i64 {
                if cancel_superseded && !req.branch.is_empty() {
                    let superseded = storage
                        .find_superseded_groups(repo_id, &req.branch, group_id)
                        .await
                        .unwrap_or_default();
                    for gid in &superseded {
                        if let Err(e) = storage
                            .update_job_group_state(
                                *gid,
                                ci_core::models::job_group::JobGroupState::Cancelled,
                            )
                            .await
                        {
                            warn!("Failed to cancel superseded group {}: {}", gid, e);
                        }
                    }
                    // Re-check count after cancellations
                    let new_count = storage
                        .count_active_groups_for_repo(repo_id)
                        .await
                        .unwrap_or(active);
                    if new_count >= max_concurrent as i64 {
                        return Err(Status::resource_exhausted(format!(
                            "Max concurrent builds ({}) reached for repo",
                            max_concurrent
                        )));
                    }
                } else {
                    return Err(Status::resource_exhausted(format!(
                        "Max concurrent builds ({}) reached for repo",
                        max_concurrent
                    )));
                }
            }
        }
    }

    // Fetch stage dependency map for DAG validation and StageInfo enrichment
    let stage_deps: std::collections::HashMap<String, Vec<String>> =
        if let Some(storage) = &state.storage {
            storage
                .get_stage_dependencies(repo_id)
                .await
                .unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

    // Validate DAG — reject reservation if stages form a cycle
    if let Err(cycle_node) = dag::validate_dag(&stage_deps) {
        return Err(Status::invalid_argument(format!(
            "Stage dependency cycle detected involving '{}'",
            cycle_node
        )));
    }

    // Helper: build StageInfo with depends_on from the DB map
    let build_stage_info = |name: &str| -> ci_core::proto::orchestrator::StageInfo {
        let deps = stage_deps.get(name).cloned().unwrap_or_default();
        ci_core::proto::orchestrator::StageInfo {
            stage_name: name.to_string(),
            command: String::new(),
            required_cpu: 0,
            required_memory_mb: 0,
            required_disk_mb: 0,
            max_duration_secs: 0,
            parallel_group: String::new(),
            job_type: "common".to_string(),
            depends_on: deps,
        }
    };

    // Idempotency: if key provided, return existing group with same key
    if !req.idempotency_key.is_empty() {
        if let Some(storage) = &state.storage {
            if let Ok(Some(existing)) = storage.find_by_idempotency_key(&req.idempotency_key).await
            {
                let worker_connected = {
                    let wr = state.worker_registry.read().await;
                    existing
                        .reserved_worker_id
                        .as_ref()
                        .and_then(|wid| wr.get(wid))
                        .map(|w| w.status == ci_core::models::worker::WorkerStatus::Connected)
                        .unwrap_or(false)
                };

                if worker_connected {
                    info!(
                        "Returning existing group {} for idempotency_key={}",
                        existing.id, req.idempotency_key
                    );
                    let stages = req.stages.iter().map(|n| build_stage_info(n)).collect();
                    return Ok(ReserveWorkerResponse {
                        job_group_id: existing.id.to_string(),
                        worker_id: existing.reserved_worker_id.unwrap_or_default(),
                        stages,
                        success: true,
                        message: "Existing active group returned (idempotency key match)"
                            .to_string(),
                    });
                } else {
                    warn!(
                        "Existing group {} (key={}) has disconnected worker, creating new reservation",
                        existing.id, req.idempotency_key
                    );
                }
            }
        }
    }
    // No key or no match -- create new group (no implicit dedup)

    // Redis lock acquired (or no Redis) -- now create the in-memory group
    let mut group = ci_core::models::job_group::JobGroup::new(
        repo_id,
        Some(req.branch.clone()).filter(|s| !s.is_empty()),
        Some(req.commit_sha.clone()).filter(|s| !s.is_empty()),
    );
    group.id = group_id;
    group.reserved_worker_id = Some(worker_id.clone());
    group.state = ci_core::models::job_group::JobGroupState::Reserved;
    group.priority = req.priority;
    group.idempotency_key = if req.idempotency_key.is_empty() {
        None
    } else {
        Some(req.idempotency_key.clone())
    };
    group.allocated_resources = ci_core::models::job_group::AllocatedResources {
        cpu: needed_cpu,
        memory_mb: needed_memory,
        disk_mb: needed_disk,
    };
    group.updated_at = chrono::Utc::now();

    // Build stage info from request stages
    let stage_infos: Vec<ci_core::proto::orchestrator::StageInfo> =
        req.stages.iter().map(|n| build_stage_info(n)).collect();

    // Persist job group to PostgreSQL (clone before add_group takes ownership)
    if let Some(storage) = &state.storage {
        if let Err(e) = storage.create_job_group(&group).await {
            warn!("Failed to persist job group {}: {}", group_id, e);
        }
    }

    // Add to job group registry (takes ownership of group)
    {
        let mut jg_registry = state.job_group_registry.write().await;
        jg_registry.add_group(group);
    }

    // Metrics
    state.metrics.inc_worker_reservations();
    state.metrics.inc_active_builds();

    info!(
        "Worker {} reserved for group {} (repo: {}, branch: {})",
        worker_id, group_id, req.repo_name, req.branch
    );

    Ok(ReserveWorkerResponse {
        job_group_id: group_id.to_string(),
        worker_id,
        stages: stage_infos,
        success: true,
        message: "Worker reserved successfully".to_string(),
    })
}

/// Core logic for the `submit_stage` RPC.
async fn do_submit_stage(
    state: &Arc<ControllerState>,
    req: SubmitStageRequest,
) -> Result<SubmitStageResponse, Status> {
    let group_id = uuid::Uuid::parse_str(&req.job_group_id)
        .map_err(|e| Status::invalid_argument(format!("Invalid job_group_id: {}", e)))?;

    // Verify the group exists and get the reserved worker
    let (worker_id, repo_id, group_priority) = {
        let jg_registry = state.job_group_registry.read().await;
        let group = jg_registry.get(&group_id).ok_or_else(|| {
            Status::not_found(format!("Job group {} not found", req.job_group_id))
        })?;
        let wid = group.reserved_worker_id.clone().ok_or_else(|| {
            Status::failed_precondition(format!(
                "Job group {} has no reserved worker",
                req.job_group_id
            ))
        })?;
        (wid, group.repo_id, group.priority)
    };

    // Check DAG dependencies: all depends_on stages must be in Success state
    {
        let depends_on: Vec<String> = if let (Some(storage), Some(rid)) = (&state.storage, repo_id)
        {
            storage
                .get_stage_dependencies(rid)
                .await
                .unwrap_or_default()
                .remove(&req.stage_name)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if !depends_on.is_empty() {
            let jg_registry = state.job_group_registry.read().await;
            if !jg_registry.can_submit_stage(&group_id, &req.stage_name, &depends_on) {
                return Err(Status::failed_precondition(format!(
                    "Stage '{}' dependencies not yet satisfied: {:?}",
                    req.stage_name, depends_on
                )));
            }
        }
    }

    // Reject if stage was already submitted in this group (any state).
    // Retries require a new reservation — workspace is cleaned up on completion.
    {
        let jg = state.job_group_registry.read().await;
        let jobs = jg.get_jobs_for_group(&group_id);
        if let Some(existing) = jobs
            .iter()
            .find(|j| j.stage_name.as_deref() == Some(&req.stage_name))
        {
            return Err(Status::new(
                tonic::Code::AlreadyExists,
                format!(
                    "Stage '{}' is already {} in group {} (retry requires a new reservation)",
                    req.stage_name, existing.state, group_id
                ),
            ));
        }
    }

    // Determine the job ID (use provided or generate)
    let job_id = if req.job_id.is_empty() {
        format!("{}-{}", group_id, req.stage_name)
    } else {
        req.job_id.clone()
    };

    // Load stage config once for labels + blacklist scope + command_mode
    let stage_config = if let (Some(storage), Some(rid)) = (&state.storage, repo_id) {
        storage
            .get_stage_config_by_name(rid, &req.stage_name)
            .await
            .unwrap_or(None)
    } else {
        None
    };
    let stage_config_id = stage_config.as_ref().map(|sc| sc.id);

    // Resolve command based on command_mode
    let user_command = &req.command_override;
    let command = if let Some(ref sc) = stage_config {
        match sc.command_mode.as_str() {
            "fixed" => {
                // Use configured command only, ignore user command
                sc.command
                    .clone()
                    .ok_or_else(|| Status::invalid_argument("Stage has no configured command"))?
            }
            "optional" => {
                // User command overrides if provided, else use configured
                if !user_command.is_empty() {
                    user_command.clone()
                } else {
                    sc.command.clone().ok_or_else(|| {
                        Status::invalid_argument(
                            "Stage has no configured command and none provided",
                        )
                    })?
                }
            }
            "required" => {
                // User MUST provide command
                if user_command.is_empty() {
                    return Err(Status::invalid_argument(
                        "This stage requires a command from the user",
                    ));
                }
                user_command.clone()
            }
            _ => sc.command.clone().unwrap_or_default(),
        }
    } else if !user_command.is_empty() {
        user_command.clone()
    } else {
        format!(
            "echo 'Running stage: {}'",
            req.stage_name.replace('\'', "'\\''")
        )
    };

    // Check command blacklist
    if let (Some(storage), Some(rid)) = (&state.storage, repo_id) {
        let patterns = storage
            .get_active_command_blacklist(rid, stage_config_id)
            .await
            .unwrap_or_default();
        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(&command) {
                    return Err(Status::permission_denied(format!(
                        "Command blocked by blacklist rule: {}",
                        pattern
                    )));
                }
            }
        }
    }

    // Load pipeline variables and inject into environment; track secret keys
    let mut environment = req.environment.clone();
    let mut secret_env_keys: Vec<String> = Vec::new();
    if let (Some(storage), Some(rid)) = (&state.storage, repo_id) {
        if let Ok(vars) = storage.list_variables_for_repo(rid).await {
            for var in vars {
                environment.entry(var.name.clone()).or_insert(var.value);
                if var.is_secret {
                    secret_env_keys.push(var.name);
                }
            }
        }
    }

    // Extract fields from stage_config before consuming it
    let (required_labels, max_duration_secs_cfg) = match &stage_config {
        Some(sc) => (sc.required_labels.clone(), sc.max_duration_secs),
        None => (Vec::new(), 0),
    };

    // Load pre/post scripts from DB for this stage + worker
    let (mut pre_script, mut post_script) =
        if let (Some(storage), Some(sc_id)) = (&state.storage, stage_config_id) {
            let scripts = storage
                .get_scripts_for_stage(sc_id, Some(&worker_id))
                .await
                .unwrap_or_default();
            let pre = scripts
                .iter()
                .find(|s| s.script_type == "pre" && s.script_scope == "worker")
                .map(|s| s.script.clone())
                .unwrap_or_default();
            let post = scripts
                .iter()
                .find(|s| s.script_type == "post" && s.script_scope == "worker")
                .map(|s| s.script.clone())
                .unwrap_or_default();
            (pre, post)
        } else {
            (String::new(), String::new())
        };

    // Prepend/append global pre/post scripts at reservation boundaries
    if let (Some(storage), Some(rid)) = (&state.storage, repo_id) {
        if let Ok((g_pre, g_pre_scope, g_post, g_post_scope)) =
            storage.get_global_scripts(rid).await
        {
            // First stage: prepend global pre_script (worker-scope).
            // Race-safe: group state transitions Reserved->Running atomically
            // under write lock when the first job is added, so only one
            // concurrent call can ever see Reserved.
            if let Some(ref g_pre_body) = g_pre {
                if matches!(g_pre_scope.as_str(), "worker" | "both") {
                    let is_first = {
                        let jg = state.job_group_registry.read().await;
                        jg.get(&group_id)
                            .map(|g| g.state == ci_core::models::job_group::JobGroupState::Reserved)
                            .unwrap_or(false)
                    };
                    if is_first {
                        if pre_script.is_empty() {
                            pre_script = g_pre_body.clone();
                        } else {
                            pre_script = format!("{}\n{}", g_pre_body, pre_script);
                        }
                    }
                }
            }
            // Last stage: append global post_script (worker-scope).
            // Race-safe: uses submitted job count vs total stage count.
            // Parallel submissions each see their own snapshot of
            // submitted_count; only the Nth caller (where N == total)
            // will satisfy the condition.
            if let Some(ref g_post_body) = g_post {
                if matches!(g_post_scope.as_str(), "worker" | "both") {
                    let is_last = {
                        let jg = state.job_group_registry.read().await;
                        let total_stages = storage
                            .get_stage_configs_for_repo(rid)
                            .await
                            .map(|s| s.len())
                            .unwrap_or(0);
                        let submitted_count = jg.get_jobs_for_group(&group_id).len();
                        // Current submission not yet added, so +1
                        total_stages > 0 && submitted_count + 1 >= total_stages
                    };
                    if is_last {
                        if post_script.is_empty() {
                            post_script = g_post_body.clone();
                        } else {
                            post_script = format!("{}\n{}", post_script, g_post_body);
                        }
                    }
                }
            }
        }
    }

    // Inject build context env vars for pre/post scripts
    environment.insert("JOB_GROUP_ID".into(), group_id.to_string());
    environment.insert("STAGE_NAME".into(), req.stage_name.clone());
    {
        let jg_registry = state.job_group_registry.read().await;
        if let Some(group) = jg_registry.get(&group_id) {
            if let Some(ref sha) = group.commit_sha {
                environment.insert("COMMIT_SHA".into(), sha.clone());
            }
            if let Some(ref branch) = group.branch {
                environment.insert("BRANCH".into(), branch.clone());
            }
        }
    }
    if let (Some(storage), Some(rid)) = (&state.storage, repo_id) {
        if let Ok(Some(repo)) = storage.get_repo(rid).await {
            environment.insert("REPO_URL".into(), repo.repo_url);
            environment.insert("REPO_NAME".into(), repo.repo_name);
        }
    }

    // Create the job
    let mut job = Job::new(
        job_id.clone(),
        command.clone(),
        JobType::Common,
        0, // required_cpu
        0, // required_memory_mb
        0, // required_disk_mb
    );
    job.job_group_id = Some(group_id);
    job.stage_name = Some(req.stage_name.clone());
    job.stage_config_id = stage_config_id;
    job.assigned_worker = Some(worker_id.clone());
    job.environment = environment.clone();
    job.state = ci_core::models::job::JobState::Queued;
    job.priority = group_priority;
    job.required_labels = required_labels;
    job.pre_script = if pre_script.is_empty() {
        None
    } else {
        Some(pre_script.clone())
    };
    job.post_script = if post_script.is_empty() {
        None
    } else {
        Some(post_script.clone())
    };
    job.max_duration_secs = if max_duration_secs_cfg > 0 {
        Some(max_duration_secs_cfg)
    } else {
        None
    };

    // Add to both registries
    {
        let mut jg_registry = state.job_group_registry.write().await;
        // Update group state to Running if it was Reserved
        if let Some(group) = jg_registry.get(&group_id) {
            if group.state == ci_core::models::job_group::JobGroupState::Reserved {
                jg_registry.update_state(
                    &group_id,
                    ci_core::models::job_group::JobGroupState::Running,
                );
            }
        }
        jg_registry.add_job_to_group(&group_id, job.clone());
        jg_registry.touch_activity(&group_id);
    }
    {
        let mut job_registry = state.job_registry.write().await;
        job_registry.add_job(job);
    }

    // Metrics
    state.metrics.inc_stages_submitted();
    state.metrics.inc_active_stages();

    // Persist job to PostgreSQL
    if let Some(storage) = &state.storage {
        let now = chrono::Utc::now();
        let db_job = crate::storage::DbJob {
            id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, job_id.as_bytes()),
            job_group_id: group_id,
            stage_config_id,
            stage_name: req.stage_name.clone(),
            command: command.clone(),
            pre_script: if pre_script.is_empty() {
                None
            } else {
                Some(pre_script.clone())
            },
            post_script: if post_script.is_empty() {
                None
            } else {
                Some(post_script.clone())
            },
            worker_id: Some(worker_id.clone()),
            state: "queued".to_string(),
            exit_code: None,
            pre_exit_code: None,
            post_exit_code: None,
            log_path: None,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = storage.create_job(&db_job).await {
            warn!("Failed to persist job {}: {}", job_id, e);
        }
    }

    // Register structured log path + UUID alias
    {
        let job_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, job_id.as_bytes());
        let mut log_agg = state.log_aggregator.write().await;
        log_agg.register_alias(job_uuid.to_string(), job_id.clone());
        log_agg.register_log_path(&job_id, &group_id.to_string(), &req.stage_name);
    }

    // Wake all waiting job_stream tasks to check for new work
    state.scheduler_notify.notify_waiters();

    // Dispatch the job to the reserved worker via job stream
    {
        let senders = state.job_stream_senders.read().await;
        if let Some(sender) = senders.get(&worker_id) {
            let assignment = JobAssignment {
                job_id: job_id.clone(),
                command,
                job_type: "common".to_string(),
                required_cpu: 0,
                required_memory_mb: 0,
                required_disk_mb: 0,
                isolation_required: false,
                branch_id: String::new(),
                environment,
                cancel: None,
                job_group_id: group_id.to_string(),
                stage_name: req.stage_name.clone(),
                pre_script,
                post_script,
                max_duration_secs: max_duration_secs_cfg,
                secret_env_keys,
            };
            if sender.send(Ok(assignment)).await.is_err() {
                warn!(
                    "Failed to send stage job {} to worker {} (channel closed)",
                    job_id, worker_id
                );
            } else {
                info!("Stage job {} dispatched to worker {}", job_id, worker_id);
            }
        } else {
            warn!(
                "No job stream channel for worker {} - job {} will be picked up on next poll",
                worker_id, job_id
            );
        }
    }

    // Refresh reservation TTL so long-running pipelines don't expire mid-build
    if let Some(redis) = &state.redis_store {
        let ttl = state.config.workers.reservation_timeout_secs;
        let gid_str = group_id.to_string();
        if let Err(e) = redis
            .refresh_reservation_ttl(&worker_id, &gid_str, ttl)
            .await
        {
            warn!("Failed to refresh reservation TTL for worker {worker_id}: {e}");
        }
    }

    Ok(SubmitStageResponse {
        job_id,
        stage_name: req.stage_name,
        accepted: true,
        message: "Stage submitted successfully".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Service definition
// ---------------------------------------------------------------------------

/// gRPC service implementation
pub struct OrchestratorService {
    state: Arc<ControllerState>,
}

// ---------------------------------------------------------------------------
// Orchestrator trait implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl Orchestrator for OrchestratorService {
    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let req = request.into_inner();
        info!("Worker registration request from: {}", req.worker_id);

        let mut registry = self.state.worker_registry.write().await;
        registry.register(&req);
        drop(registry);

        // Worker availability is determined by in-memory resource tracking,
        // not a Redis set. No need to manage an "available workers" set.

        Ok(Response::new(RegisterResponse {
            accepted: true,
            message: "Worker registered successfully".to_string(),
            heartbeat_interval_secs: self.state.config.workers.heartbeat_interval_secs,
        }))
    }

    type HeartbeatStream = tokio_stream::wrappers::ReceiverStream<Result<HeartbeatAck, Status>>;

    async fn heartbeat(
        &self,
        request: Request<tonic::Streaming<HeartbeatMessage>>,
    ) -> Result<Response<Self::HeartbeatStream>, Status> {
        let mut stream = request.into_inner();
        let state = self.state.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            while let Ok(Some(msg)) = stream.message().await {
                let mut registry = state.worker_registry.write().await;
                registry.update_heartbeat(&msg);

                let ack = HeartbeatAck {
                    ok: true,
                    message: "ok".to_string(),
                };
                if tx.send(Ok(ack)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    type JobStreamStream = tokio_stream::wrappers::ReceiverStream<Result<JobAssignment, Status>>;

    async fn job_stream(
        &self,
        request: Request<JobStreamRequest>,
    ) -> Result<Response<Self::JobStreamStream>, Status> {
        let req = request.into_inner();
        let worker_id = req.worker_id.clone();
        info!("Worker {} requesting job stream", worker_id);

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let state = self.state.clone();

        // Register this worker's channel for job assignments (including cancel directives)
        {
            let mut senders = self.state.job_stream_senders.write().await;
            senders.insert(worker_id.clone(), tx.clone());
            info!("Registered job stream channel for worker {}", worker_id);
        }

        // Clone tx for the job assignment loop
        let job_tx = tx.clone();

        tokio::spawn(async move {
            // Job assignment loop: wake on scheduler notify or 30s fallback
            loop {
                tokio::select! {
                    _ = state.scheduler_notify.notified() => {}
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {}
                }

                // Check if channel is still open
                if job_tx.is_closed() {
                    info!("Job stream channel closed for worker {}", worker_id);
                    break;
                }

                if !dispatch_job_for_worker(&state, &worker_id, &job_tx).await {
                    break;
                }
            }

            // Cleanup: remove from job_stream_senders when stream ends
            let mut senders = state.job_stream_senders.write().await;
            senders.remove(&worker_id);
            info!("Removed job stream channel for worker {}", worker_id);
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn report_job_status(
        &self,
        request: Request<JobStatusUpdate>,
    ) -> Result<Response<JobStatusAck>, Status> {
        let req = request.into_inner();
        info!(
            "Job status update: job={} worker={} state={:?}",
            req.job_id, req.worker_id, req.state
        );

        let mut registry = self.state.job_registry.write().await;
        registry.update_status(&req);
        drop(registry);

        // Metrics based on reported state
        let reported_state = req.state;
        if reported_state == ci_core::proto::orchestrator::JobState::Success as i32 {
            self.state.metrics.inc_jobs_completed();
            self.state.metrics.dec_active_stages();
        } else if reported_state == ci_core::proto::orchestrator::JobState::Failed as i32 {
            self.state.metrics.inc_jobs_failed();
            self.state.metrics.inc_stage_failures();
            self.state.metrics.dec_active_stages();
        } else if reported_state == ci_core::proto::orchestrator::JobState::Cancelled as i32 {
            self.state.metrics.inc_jobs_cancelled();
            self.state.metrics.dec_active_stages();
        }

        // Persist job state to DB
        if let Some(storage) = &self.state.storage {
            let state_str = match reported_state {
                x if x == ci_core::proto::orchestrator::JobState::Running as i32 => "running",
                x if x == ci_core::proto::orchestrator::JobState::Success as i32 => "success",
                x if x == ci_core::proto::orchestrator::JobState::Failed as i32 => "failed",
                x if x == ci_core::proto::orchestrator::JobState::Cancelled as i32 => "cancelled",
                x if x == ci_core::proto::orchestrator::JobState::Assigned as i32 => "assigned",
                _ => "unknown",
            };
            // DB uses deterministic UUID v5 derived from job_id string
            // (same as what do_submit_stage uses when persisting the job)
            let job_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, req.job_id.as_bytes());
            if let Err(e) = storage
                .update_job_state(
                    job_uuid,
                    state_str,
                    if req.exit_code != 0 || state_str == "success" || state_str == "failed" {
                        Some(req.exit_code)
                    } else {
                        None
                    },
                    None,
                    None,
                    Some(&req.worker_id),
                )
                .await
            {
                warn!("Failed to update job state in DB: {}", e);
            }
        }

        // Record resource usage for terminal states
        let is_terminal = reported_state == ci_core::proto::orchestrator::JobState::Success as i32
            || reported_state == ci_core::proto::orchestrator::JobState::Failed as i32
            || reported_state == ci_core::proto::orchestrator::JobState::Cancelled as i32;
        if is_terminal {
            if let Some(storage) = &self.state.storage {
                let registry = self.state.job_registry.read().await;
                if let Some(job) = registry.get(&req.job_id) {
                    if let Some(stage_config_id) = job.stage_config_id {
                        let repo_id = if let Some(group_id) = job.job_group_id {
                            let jg = self.state.job_group_registry.read().await;
                            jg.get(&group_id)
                                .and_then(|g| g.repo_id)
                                .unwrap_or_default()
                        } else {
                            uuid::Uuid::nil()
                        };
                        let job_uuid = uuid::Uuid::parse_str(&req.job_id).unwrap_or_default();
                        let duration_secs = job.started_at.map(|start| {
                            chrono::Utc::now()
                                .signed_duration_since(start)
                                .num_seconds() as i32
                        });
                        if let Err(e) = storage
                            .record_stage_resources(
                                stage_config_id,
                                repo_id,
                                job_uuid,
                                None, // cpu — not tracked in heartbeat yet
                                None, // memory — not tracked in heartbeat yet
                                None, // disk — not tracked in heartbeat yet
                                duration_secs,
                                Some(req.exit_code),
                            )
                            .await
                        {
                            warn!(
                                "Failed to record stage resources for job {}: {e}",
                                req.job_id
                            );
                        }
                    }
                }
            }
        }

        // Check group completion
        if !req.job_group_id.is_empty() {
            if let Ok(group_id) = uuid::Uuid::parse_str(&req.job_group_id) {
                // Phase 1: update state under lock, collect results (no .await)
                let completion_info = {
                    let mut jg = self.state.job_group_registry.write().await;

                    // Update job state in group_jobs so check_group_completion sees current state
                    let model_state = match reported_state {
                        x if x == ci_core::proto::orchestrator::JobState::Success as i32 => {
                            ci_core::models::job::JobState::Success
                        }
                        x if x == ci_core::proto::orchestrator::JobState::Failed as i32 => {
                            ci_core::models::job::JobState::Failed
                        }
                        x if x == ci_core::proto::orchestrator::JobState::Cancelled as i32 => {
                            ci_core::models::job::JobState::Cancelled
                        }
                        x if x == ci_core::proto::orchestrator::JobState::Running as i32 => {
                            ci_core::models::job::JobState::Running
                        }
                        _ => ci_core::models::job::JobState::Unknown,
                    };
                    jg.update_job_in_group(
                        &group_id,
                        &req.job_id,
                        model_state,
                        Some(req.exit_code),
                    );
                    jg.touch_activity(&group_id);

                    jg.check_group_completion(&group_id).map(|new_state| {
                        let worker_id = jg.on_group_completed(&group_id);
                        let repo_id = jg.get(&group_id).and_then(|g| g.repo_id);
                        let branch = jg.get(&group_id).and_then(|g| g.branch.clone());
                        let commit_sha = jg.get(&group_id).and_then(|g| g.commit_sha.clone());
                        let alloc = jg
                            .get(&group_id)
                            .map(|g| g.allocated_resources)
                            .unwrap_or_default();
                        (new_state, worker_id, repo_id, branch, commit_sha, alloc)
                    })
                }; // jg write lock dropped here

                // Phase 2: async I/O without holding the lock
                if let Some((new_state, worker_id, repo_id, branch, commit_sha, alloc)) =
                    completion_info
                {
                    info!("Job group {} completed: {}", group_id, new_state);
                    self.state.metrics.dec_active_builds();

                    if let Some(ref wid) = worker_id {
                        // Release allocated resources on the worker
                        if alloc.cpu > 0 || alloc.memory_mb > 0 || alloc.disk_mb > 0 {
                            let mut wr = self.state.worker_registry.write().await;
                            if let Some(w) = wr.get_mut(wid) {
                                w.release(alloc.cpu, alloc.memory_mb, alloc.disk_mb);
                            }
                        }

                        if let Some(redis) = &self.state.redis_store {
                            if let Err(e) = ReservationManager::release(redis, wid, &group_id).await
                            {
                                warn!("Failed to release reservation for worker {}: {}", wid, e);
                            }
                        }
                    }

                    if let Some(storage) = &self.state.storage {
                        if let Err(e) = storage.update_job_group_state(group_id, new_state).await {
                            error!("Failed to persist group {} state to DB: {e}", group_id);
                        }

                        // Dispatch notifications for completed group
                        if let Some(repo_id) = repo_id {
                            let event_type = if new_state
                                == ci_core::models::job_group::JobGroupState::Success
                            {
                                "on_success"
                            } else {
                                "on_failure"
                            };
                            let payload = serde_json::json!({
                                "group_id": group_id.to_string(),
                                "repo": repo_id.to_string(),
                                "branch": branch,
                                "commit_sha": commit_sha,
                                "state": new_state.to_string(),
                            });
                            let storage = storage.clone();
                            let event = event_type.to_string();
                            tokio::spawn(async move {
                                crate::notifier::dispatch_notifications(
                                    &storage, repo_id, &event, payload,
                                )
                                .await;
                            });
                        }
                    }
                }
            }
        }

        Ok(Response::new(JobStatusAck {
            ok: true,
            message: "Status updated".to_string(),
        }))
    }

    async fn stream_logs(
        &self,
        request: Request<tonic::Streaming<LogChunk>>,
    ) -> Result<Response<LogAck>, Status> {
        let mut stream = request.into_inner();
        let state = self.state.clone();

        // Spawn a task to consume the stream and feed chunks to LogAggregator
        let handle = tokio::spawn(async move {
            let mut last_job_id = String::new();
            let mut last_offset = 0u64;

            while let Ok(Some(chunk)) = stream.message().await {
                last_job_id = chunk.job_id.clone();
                let mut aggregator = state.log_aggregator.write().await;
                last_offset = aggregator.append_chunk(
                    &chunk.job_id,
                    &chunk.worker_id,
                    chunk.offset,
                    &chunk.data,
                    chunk.timestamp_unix,
                );
            }

            (last_job_id, last_offset)
        });

        // Wait for the stream to complete
        match handle.await {
            Ok((job_id, offset)) => {
                // Mark the job's log stream as complete
                let mut aggregator = self.state.log_aggregator.write().await;
                aggregator.finalize(&job_id);
                Ok(Response::new(LogAck {
                    job_id,
                    last_offset: offset,
                }))
            }
            Err(e) => {
                error!("Stream logs task failed: {}", e);
                Err(Status::internal("Log streaming failed"))
            }
        }
    }

    async fn reconnect(
        &self,
        request: Request<ReconnectRequest>,
    ) -> Result<Response<ReconnectResponse>, Status> {
        let req = request.into_inner();
        warn!(
            "Worker {} reconnecting with {} running jobs",
            req.worker_id,
            req.running_jobs.len()
        );

        // Re-register the worker (update heartbeat and mark as active)
        {
            let mut registry = self.state.worker_registry.write().await;
            registry.mark_reconnected(&req.worker_id);
        }

        // Build log resume directives by reconciling job state
        let log_resumes = {
            let job_registry = self.state.job_registry.read().await;
            let log_aggregator = self.state.log_aggregator.read().await;

            let mut directives = Vec::new();

            for running_job in &req.running_jobs {
                // Verify this job is still assigned to this worker
                if let Some(job) = job_registry.get(&running_job.job_id) {
                    if job.assigned_worker == Some(req.worker_id.clone()) {
                        // Job is legitimately assigned to this worker
                        // Check what log offset the controller has
                        let controller_offset = log_aggregator.last_offset(&running_job.job_id);

                        // Worker should resume from where controller left off
                        // (in case some logs were lost during disconnect)
                        directives.push(LogResumeDirective {
                            job_id: running_job.job_id.clone(),
                            resume_from_offset: controller_offset,
                        });
                    } else {
                        // Job was reassigned to another worker - mark as conflict
                        warn!(
                            "Job {} was reassigned from reconnected worker {}",
                            running_job.job_id, req.worker_id
                        );
                        // The worker should stop this job - we could add a field for this
                    }
                } else {
                    // Job no longer exists in registry - worker should stop it
                    warn!(
                        "Job {} no longer exists, worker {} should stop",
                        running_job.job_id, req.worker_id
                    );
                }
            }

            directives
        };

        info!(
            "Worker {} reconnected, returning {} log resume directives",
            req.worker_id,
            log_resumes.len()
        );

        Ok(Response::new(ReconnectResponse {
            accepted: true,
            message: "Reconnection accepted".to_string(),
            log_resumes,
        }))
    }

    async fn get_job_status(
        &self,
        request: Request<GetJobStatusRequest>,
    ) -> Result<Response<GetJobStatusResponse>, Status> {
        let req = request.into_inner();
        let registry = self.state.job_registry.read().await;

        match registry.get(&req.job_id) {
            Some(job) => {
                let state = job_state_to_proto(job.state);

                // Get log data from aggregator
                let log_aggregator = self.state.log_aggregator.read().await;
                let log_output = log_aggregator.get_log_string(&req.job_id);
                drop(log_aggregator);

                // Prefer stored output if available, otherwise use streamed logs
                let output = job.output.clone().unwrap_or(log_output);

                Ok(Response::new(GetJobStatusResponse {
                    found: true,
                    job_id: job.job_id.clone(),
                    state,
                    message: format!("Job state: {}", job.state),
                    exit_code: job.exit_code.unwrap_or(0),
                    output,
                }))
            }
            None => Ok(Response::new(GetJobStatusResponse {
                found: false,
                job_id: req.job_id,
                state: ci_core::proto::orchestrator::JobState::Unknown as i32,
                message: "Job not found".to_string(),
                exit_code: 0,
                output: String::new(),
            })),
        }
    }

    #[tracing::instrument(skip(self, request), fields(job_id = %request.get_ref().job_id))]
    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> Result<Response<SubmitJobResponse>, Status> {
        // Get peer address for orphan detection BEFORE consuming request
        let peer_addr = request
            .remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();

        let req = request.into_inner();
        info!("Job submission: {} - {}", req.job_id, req.command);

        // Parse job type
        let job_type = match req.job_type.as_str() {
            "heavy" => JobType::Heavy,
            "nix" => JobType::Nix,
            "test" => JobType::Test,
            _ => JobType::Common,
        };

        // Create job with an ad-hoc group ID so completion tracking works
        let branch = Some(req.branch_id.clone()).filter(|s| !s.is_empty());
        let group_id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, req.job_id.as_bytes());
        let mut job = Job::new(
            req.job_id.clone(),
            req.command.clone(),
            job_type,
            req.required_cpu,
            req.required_memory_mb,
            req.required_disk_mb,
        );
        job.isolation_required = req.isolation_required;
        job.branch_id = branch.clone();
        job.environment = req.environment;
        job.submitter_connection_id = Some(peer_addr);
        job.job_group_id = Some(group_id);

        // Add to registries (both job + group so completion detection works)
        {
            let mut jgr = self.state.job_group_registry.write().await;
            let group = ci_core::models::job_group::JobGroup::new_with_id(
                group_id,
                None,
                branch.clone(),
                None,
                "cli".to_string(),
            );
            jgr.add_group(group);
            jgr.add_job_to_group(&group_id, job.clone());
        }
        {
            let mut registry = self.state.job_registry.write().await;
            registry.add_job(job);
        }

        // Register UUID alias + structured log path
        {
            let job_uuid = uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_OID,
                format!("job-{}", req.job_id).as_bytes(),
            );
            let mut log_agg = self.state.log_aggregator.write().await;
            log_agg.register_alias(job_uuid.to_string(), req.job_id.clone());
            log_agg.register_log_path(&req.job_id, &group_id.to_string(), "default");
        }

        self.state.metrics.inc_jobs_submitted();
        self.state.metrics.inc_active_stages();

        // Persist to DB (ad-hoc job group + job)
        if let Some(storage) = &self.state.storage {
            let now = chrono::Utc::now();
            let group_id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, req.job_id.as_bytes());

            let group = ci_core::models::job_group::JobGroup {
                id: group_id,
                repo_id: None,
                branch: branch.clone(),
                commit_sha: None,
                trigger_source: "cli".to_string(),
                reserved_worker_id: None,
                state: ci_core::models::job_group::JobGroupState::Running,
                priority: 0,
                pr_number: None,
                idempotency_key: None,
                allocated_resources: ci_core::models::job_group::AllocatedResources::default(),
                created_at: now,
                updated_at: now,
                completed_at: None,
                last_activity_at: now,
            };
            if let Err(e) = storage.create_job_group(&group).await {
                warn!("Failed to persist ad-hoc job group: {}", e);
            }

            let db_job = crate::storage::DbJob {
                id: uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_OID,
                    format!("job-{}", req.job_id).as_bytes(),
                ),
                job_group_id: group_id,
                stage_config_id: None,
                stage_name: "default".to_string(),
                command: req.command.clone(),
                pre_script: None,
                post_script: None,
                worker_id: None,
                state: "queued".to_string(),
                exit_code: None,
                pre_exit_code: None,
                post_exit_code: None,
                log_path: None,
                started_at: None,
                completed_at: None,
                retry_count: 0,
                created_at: now,
                updated_at: now,
            };
            if let Err(e) = storage.create_job(&db_job).await {
                warn!("Failed to persist ad-hoc job: {}", e);
            }
        }

        // Wake all waiting job_stream tasks to check for new work
        self.state.scheduler_notify.notify_waiters();

        Ok(Response::new(SubmitJobResponse {
            accepted: true,
            message: "Job queued successfully".to_string(),
            job_id: req.job_id,
        }))
    }

    type WatchJobLogsStream = tokio_stream::wrappers::ReceiverStream<Result<LogChunk, Status>>;

    async fn watch_job_logs(
        &self,
        request: Request<WatchJobLogsRequest>,
    ) -> Result<Response<Self::WatchJobLogsStream>, Status> {
        let req = request.into_inner();
        let job_id = req.job_id.clone();
        let from_offset = req.from_offset;

        info!(
            "WatchJobLogs request: job_id={}, from_offset={}",
            job_id, from_offset
        );

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let state = self.state.clone();

        tokio::spawn(async move {
            // Subscribe to log updates
            let (buffered, mut live_rx, initially_complete) = {
                let mut aggregator = state.log_aggregator.write().await;
                aggregator.subscribe(&job_id, from_offset)
            };

            // Send buffered data first
            for chunk in buffered {
                let log_chunk = LogChunk {
                    worker_id: chunk.worker_id,
                    job_id: job_id.clone(),
                    offset: chunk.offset,
                    data: chunk.data,
                    timestamp_unix: chunk.timestamp_unix,
                };
                if tx.send(Ok(log_chunk)).await.is_err() {
                    info!("WatchJobLogs client disconnected during buffered send");
                    return;
                }
            }

            // If already complete, close the stream
            if initially_complete {
                info!("WatchJobLogs: job {} logs already complete", job_id);
                return;
            }

            // Stream live updates until job reaches terminal state
            loop {
                tokio::select! {
                    // Receive live log chunk
                    result = live_rx.recv() => {
                        match result {
                            Ok(chunk) => {
                                let log_chunk = LogChunk {
                                    worker_id: chunk.worker_id,
                                    job_id: job_id.clone(),
                                    offset: chunk.offset,
                                    data: chunk.data,
                                    timestamp_unix: chunk.timestamp_unix,
                                };
                                if tx.send(Ok(log_chunk)).await.is_err() {
                                    info!("WatchJobLogs client disconnected");
                                    return;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("WatchJobLogs: broadcast channel closed for job {}", job_id);
                                return;
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("WatchJobLogs: client lagged by {} messages for job {}", n, job_id);
                                // Continue - the client will get next messages
                            }
                        }
                    }

                    // Periodically check if job is complete
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                        let job_registry = state.job_registry.read().await;
                        let log_aggregator = state.log_aggregator.read().await;

                        let job_complete = job_registry.get(&job_id).map(|job| {
                            matches!(
                                job.state,
                                ci_core::models::job::JobState::Success
                                    | ci_core::models::job::JobState::Failed
                                    | ci_core::models::job::JobState::Cancelled
                            )
                        }).unwrap_or(false);

                        let log_complete = log_aggregator.is_complete(&job_id);

                        if job_complete && log_complete {
                            info!("WatchJobLogs: job {} reached terminal state", job_id);
                            return;
                        }
                    }
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    #[tracing::instrument(skip(self, request), fields(job_id = %request.get_ref().job_id, reason = %request.get_ref().reason))]
    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let req = request.into_inner();
        info!(
            "CancelJob request: job_id={}, job_group_id={}, reason={}",
            req.job_id, req.job_group_id, req.reason
        );

        // If job_group_id is provided and job_id is empty, cancel the entire group
        if !req.job_group_id.is_empty() && req.job_id.is_empty() {
            let group_id = uuid::Uuid::parse_str(&req.job_group_id)
                .map_err(|_| Status::invalid_argument("Invalid job_group_id UUID"))?;

            let release_info = {
                let mut jg = self.state.job_group_registry.write().await;
                if let Some(group) = jg.get(&group_id) {
                    if group.state.is_terminal() {
                        return Ok(Response::new(CancelJobResponse {
                            accepted: false,
                            message: "Job group already in terminal state".to_string(),
                        }));
                    }
                } else {
                    return Err(Status::not_found(format!(
                        "Job group {} not found",
                        req.job_group_id
                    )));
                }
                let info = jg
                    .get(&group_id)
                    .map(|g| (g.reserved_worker_id.clone(), g.allocated_resources));
                jg.update_state(
                    &group_id,
                    ci_core::models::job_group::JobGroupState::Cancelled,
                );
                jg.fail_group_jobs(&group_id, &req.reason);
                info
            };

            // Release worker resources
            if let Some((Some(ref wid), alloc)) = release_info {
                if alloc.cpu > 0 || alloc.memory_mb > 0 || alloc.disk_mb > 0 {
                    let mut wr = self.state.worker_registry.write().await;
                    if let Some(w) = wr.get_mut(wid) {
                        w.release(alloc.cpu, alloc.memory_mb, alloc.disk_mb);
                    }
                }
                // Release Redis reservation
                if let Some(redis) = &self.state.redis_store {
                    if let Err(e) =
                        crate::reservation::ReservationManager::release(redis, wid, &group_id).await
                    {
                        warn!("Failed to release Redis reservation for worker {wid}: {e}");
                    }
                }
            }

            // Persist to DB
            if let Some(storage) = &self.state.storage {
                if let Err(e) = storage
                    .update_job_group_state(
                        group_id,
                        ci_core::models::job_group::JobGroupState::Cancelled,
                    )
                    .await
                {
                    warn!("Failed to persist cancel for group {group_id}: {e}");
                }
            }

            return Ok(Response::new(CancelJobResponse {
                accepted: true,
                message: format!("Job group {} cancelled", req.job_group_id),
            }));
        }

        // Cancel individual job by job_id
        let worker_id = {
            let mut registry = self.state.job_registry.write().await;
            registry.cancel_job(&req.job_id, &req.reason)
        };

        match worker_id {
            Some(worker_id) => {
                // Send cancel directive to the worker via the job stream
                let senders = self.state.job_stream_senders.read().await;
                if let Some(sender) = senders.get(&worker_id) {
                    let cancel_assignment = build_cancel_assignment(&req.job_id, &req.reason);
                    if sender.send(Ok(cancel_assignment)).await.is_err() {
                        warn!("Failed to send cancel directive to worker {}", worker_id);
                    } else {
                        info!(
                            "Sent cancel directive for job {} to worker {}",
                            req.job_id, worker_id
                        );
                    }
                } else {
                    warn!("No job stream channel found for worker {}", worker_id);
                }

                Ok(Response::new(CancelJobResponse {
                    accepted: true,
                    message: format!("Job {} cancelled", req.job_id),
                }))
            }
            None => {
                // Job not found or already in terminal state
                let registry = self.state.job_registry.read().await;
                if registry.get(&req.job_id).is_none() {
                    Err(Status::not_found(format!("Job {} not found", req.job_id)))
                } else {
                    Ok(Response::new(CancelJobResponse {
                        accepted: false,
                        message: "Job already in terminal state".to_string(),
                    }))
                }
            }
        }
    }

    #[tracing::instrument(skip(self, request), fields(repo = %request.get_ref().repo_name, branch = %request.get_ref().branch))]
    async fn reserve_worker(
        &self,
        request: Request<ReserveWorkerRequest>,
    ) -> Result<Response<ReserveWorkerResponse>, Status> {
        let req = request.into_inner();
        info!(
            "ReserveWorker request: repo={} branch={} stages={:?}",
            req.repo_name, req.branch, req.stages
        );

        let response = do_reserve_worker(&self.state, &req).await?;
        Ok(Response::new(response))
    }

    #[tracing::instrument(skip(self, request), fields(group = %request.get_ref().job_group_id, stage = %request.get_ref().stage_name))]
    async fn submit_stage(
        &self,
        request: Request<SubmitStageRequest>,
    ) -> Result<Response<SubmitStageResponse>, Status> {
        let req = request.into_inner();
        info!(
            "SubmitStage request: group={} stage={} job_id={}",
            req.job_group_id, req.stage_name, req.job_id
        );

        let response = do_submit_stage(&self.state, req).await?;
        Ok(Response::new(response))
    }

    async fn get_job_group_status(
        &self,
        request: Request<GetJobGroupStatusRequest>,
    ) -> Result<Response<GetJobGroupStatusResponse>, Status> {
        let req = request.into_inner();
        info!("GetJobGroupStatus request: group={}", req.job_group_id);

        let group_id = uuid::Uuid::parse_str(&req.job_group_id)
            .map_err(|e| Status::invalid_argument(format!("Invalid job_group_id: {}", e)))?;

        let jg_registry = self.state.job_group_registry.read().await;

        if let Some(group) = jg_registry.get(&group_id) {
            let jobs = jg_registry.get_jobs_for_group(&group_id);

            let stage_statuses: Vec<ci_core::proto::orchestrator::StageStatus> = jobs
                .iter()
                .map(|job| {
                    let state_str = job.state.to_string();
                    ci_core::proto::orchestrator::StageStatus {
                        job_id: job.job_id.clone(),
                        stage_name: job.stage_name.clone().unwrap_or_default(),
                        state: state_str,
                        exit_code: job.exit_code.unwrap_or(0),
                        worker_id: job.assigned_worker.clone().unwrap_or_default(),
                        started_at: job.started_at.map(|t| t.timestamp()).unwrap_or(0),
                        completed_at: job.completed_at.map(|t| t.timestamp()).unwrap_or(0),
                    }
                })
                .collect();

            return Ok(Response::new(GetJobGroupStatusResponse {
                job_group_id: group_id.to_string(),
                state: group.state.to_string(),
                worker_id: group.reserved_worker_id.clone().unwrap_or_default(),
                stages: stage_statuses,
            }));
        }
        drop(jg_registry);

        // Fallback: check PostgreSQL for persisted state
        if let Some(storage) = &self.state.storage {
            if let Ok(Some(group)) = storage.get_job_group(group_id).await {
                return Ok(Response::new(GetJobGroupStatusResponse {
                    job_group_id: group_id.to_string(),
                    state: group.state.to_string(),
                    worker_id: group.reserved_worker_id.unwrap_or_default(),
                    stages: Vec::new(), // DB jobs not mapped to StageStatus here
                }));
            }
        }

        Err(Status::not_found(format!(
            "Job group {} not found",
            req.job_group_id
        )))
    }
}

/// Start the gRPC server.
///
/// Accepts the fully-constructed shared `ControllerState` so both the gRPC and
/// HTTP servers observe the same live data.
pub async fn run(state: Arc<ControllerState>) -> anyhow::Result<()> {
    let addr = state.config.bind_address.parse()?;

    // ── Heartbeat timeout detection background task ───────────────────────────
    {
        let state_for_hb = state.clone();
        let hb_timeout = state.config.workers.heartbeat_timeout_secs as u64;
        tokio::spawn(async move {
            let check_interval = std::cmp::max(hb_timeout / 2, 1);
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(check_interval));
            loop {
                interval.tick().await;
                let timed_out = {
                    let mut registry = state_for_hb.worker_registry.write().await;
                    registry.check_heartbeat_timeouts(hb_timeout)
                };
                if !timed_out.is_empty() {
                    // Update connected-workers gauge
                    let connected = {
                        let registry = state_for_hb.worker_registry.read().await;
                        registry.connected_workers().len() as i64
                    };
                    state_for_hb.metrics.set_connected_workers(connected);

                    // Mark jobs as unknown for every dead worker and decrement active_stages
                    {
                        let mut job_registry = state_for_hb.job_registry.write().await;
                        for worker_id in &timed_out {
                            let marked = job_registry.mark_unknown_for_worker(worker_id);
                            for _ in 0..marked {
                                state_for_hb.metrics.dec_active_stages();
                            }
                        }
                    }

                    // Handle group failure for dead workers
                    for worker_id in &timed_out {
                        let mut db_updates: Vec<uuid::Uuid> = Vec::new();

                        {
                            let mut jg = state_for_hb.job_group_registry.write().await;
                            let (to_migrate, to_fail) = jg.handle_worker_death(worker_id);

                            for gid in &to_fail {
                                jg.fail_group_jobs(gid, &format!("Worker {} died", worker_id));
                                state_for_hb.metrics.dec_active_builds();
                                db_updates.push(*gid);
                            }

                            if !to_migrate.is_empty() {
                                warn!(
                                    "{} groups need migration from dead worker {} (not yet implemented)",
                                    to_migrate.len(),
                                    worker_id
                                );
                            }
                        }
                        // Lock released -- now do async DB/Redis calls

                        for gid in db_updates {
                            if let Some(storage) = &state_for_hb.storage {
                                if let Err(e) = storage
                                    .update_job_group_state(
                                        gid,
                                        ci_core::models::job_group::JobGroupState::Failed,
                                    )
                                    .await
                                {
                                    error!("Failed to mark group {gid} as failed after worker death: {e}");
                                }
                            }
                        }

                        if let Some(redis) = &state_for_hb.redis_store {
                            if let Err(e) =
                                ReservationManager::release_force(redis, worker_id).await
                            {
                                warn!("Failed to release reservation for dead worker {worker_id}: {e}");
                            }
                        }
                    }
                }
            }
        });
    }

    // ── Orphan job cleanup background task ────────────────────────────────────
    {
        let state_for_orphan = state.clone();
        let orphan_timeout = state.config.jobs.orphan_timeout_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut registry = state_for_orphan.job_registry.write().await;
                let count = registry.cancel_orphaned_jobs(orphan_timeout);
                if count > 0 {
                    warn!("Cancelled {} orphaned jobs", count);
                }
            }
        });
    }

    // ── Stuck group reaper — every 5 minutes ─────────────────────────────────
    {
        let state_reaper = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                let mut jgr = state_reaper.job_group_registry.write().await;
                let now = chrono::Utc::now();
                let stuck: Vec<uuid::Uuid> = jgr
                    .active_groups()
                    .iter()
                    .filter(|g| {
                        matches!(g.state, ci_core::models::job_group::JobGroupState::Running)
                            && (now - g.updated_at).num_hours() >= 4
                    })
                    .map(|g| g.id)
                    .collect();
                for gid in stuck {
                    warn!("Failing stuck group {} (running > 4h)", gid);
                    jgr.update_state(&gid, ci_core::models::job_group::JobGroupState::Failed);
                }
            }
        });
    }

    // ── Log buffer cleanup background task ────────────────────────────────────
    {
        let state_for_log_cleanup = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
            loop {
                interval.tick().await;
                let mut la = state_for_log_cleanup.log_aggregator.write().await;
                la.cleanup_old_logs(3600); // clean buffers finalized >1h ago
            }
        });
    }

    // ── Terminal entry eviction — every 10 minutes ────────────────────────────
    {
        let state_evict = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
            loop {
                interval.tick().await;
                let evicted_jobs = {
                    let mut jr = state_evict.job_registry.write().await;
                    jr.evict_terminal(std::time::Duration::from_secs(1800))
                };
                let evicted_groups = {
                    let mut jgr = state_evict.job_group_registry.write().await;
                    jgr.evict_terminal(std::time::Duration::from_secs(1800))
                };
                if evicted_jobs > 0 || evicted_groups > 0 {
                    info!(
                        jobs = evicted_jobs,
                        groups = evicted_groups,
                        "Evicted terminal entries from memory"
                    );
                }
            }
        });
    }

    // ── Cron/scheduled builds — every 60 seconds ──────────────────────────────
    {
        let state_cron = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let storage = match &state_cron.storage {
                    Some(s) => s.clone(),
                    None => continue,
                };
                let due = match storage.list_due_schedules().await {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Cron: failed to list due schedules: {}", e);
                        continue;
                    }
                };
                for schedule in due {
                    info!(
                        "Cron: triggering schedule {} for repo {}",
                        schedule.id, schedule.repo_id
                    );
                    // Look up repo name for the reserve request
                    let repo = match storage.get_repo(schedule.repo_id).await {
                        Ok(Some(r)) => r,
                        Ok(None) => {
                            warn!("Cron: repo {} not found, skipping", schedule.repo_id);
                            continue;
                        }
                        Err(e) => {
                            warn!("Cron: repo lookup failed: {}", e);
                            continue;
                        }
                    };
                    // Build a ReserveWorkerRequest and reuse do_reserve_worker
                    let req = ReserveWorkerRequest {
                        repo_name: repo.repo_name.clone(),
                        repo_url: repo.repo_url.clone(),
                        branch: schedule.branch.clone(),
                        commit_sha: String::new(), // cron builds use latest
                        stages: schedule.stages.clone(),
                        priority: 0,
                        idempotency_key: String::new(),
                    };
                    match do_reserve_worker(&state_cron, &req).await {
                        Ok(resp) if resp.success => {
                            info!(
                                "Cron: reserved worker {} for group {} (schedule {})",
                                resp.worker_id, resp.job_group_id, schedule.id
                            );
                        }
                        Ok(resp) => {
                            warn!(
                                "Cron: reserve failed for schedule {}: {}",
                                schedule.id, resp.message
                            );
                        }
                        Err(e) => {
                            warn!("Cron: reserve error for schedule {}: {}", schedule.id, e);
                        }
                    }
                    // Mark triggered regardless so we don't retry immediately
                    if let Err(e) = storage.mark_schedule_triggered(schedule.id).await {
                        error!(
                            "Cron: failed to mark schedule {} triggered: {}",
                            schedule.id, e
                        );
                    }
                }
            }
        });
    }

    let tls_config = state.config.tls.clone();
    let auth_config = state.config.auth.clone();
    let service = OrchestratorService { state };
    let interceptor = auth_interceptor(auth_config);
    let grpc_service = OrchestratorServer::new(service)
        .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
        .send_compressed(tonic::codec::CompressionEncoding::Gzip);
    let server = InterceptedService::new(grpc_service, interceptor);

    info!("Controller gRPC server listening on {}", addr);

    let mut server_builder = tonic::transport::Server::builder()
        .http2_keepalive_interval(Some(Duration::from_secs(10)))
        .http2_keepalive_timeout(Some(Duration::from_secs(20)))
        .tcp_keepalive(Some(Duration::from_secs(60)));

    // Configure TLS if enabled
    if let Some(ref tls) = tls_config {
        if tls.enabled {
            let cert = tokio::fs::read(
                tls.server_cert
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("server_cert required when TLS is enabled"))?,
            )
            .await?;
            let key = tokio::fs::read(
                tls.server_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("server_key required when TLS is enabled"))?,
            )
            .await?;

            let mut tls_config = tonic::transport::ServerTlsConfig::new()
                .identity(tonic::transport::Identity::from_pem(cert, key));

            // Add CA cert for client verification (mTLS)
            if let Some(ref ca_path) = tls.ca_cert {
                let ca = tokio::fs::read(ca_path).await?;
                let ca_cert = tonic::transport::Certificate::from_pem(ca);
                tls_config = tls_config.client_ca_root(ca_cert);
            }

            server_builder = server_builder.tls_config(tls_config)?;
            info!("TLS enabled for gRPC server");
        }
    }

    server_builder.add_service(server).serve(addr).await?;

    Ok(())
}
