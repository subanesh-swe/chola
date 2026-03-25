use std::sync::Arc;
use tokio::sync::{broadcast, Notify, RwLock};
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use ci_core::models::config::ControllerConfig;
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

use crate::job_group_registry::JobGroupRegistry;
use crate::job_registry::JobRegistry;
use crate::log_aggregator::LogAggregator;
use crate::monitoring::Metrics;
use crate::scheduler::{BestFitScheduler, Scheduler};
use crate::worker_registry::WorkerRegistry;

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
async fn dispatch_job_for_worker(
    state: &Arc<ControllerState>,
    worker_id: &str,
    job_tx: &tokio::sync::mpsc::Sender<Result<JobAssignment, Status>>,
) -> bool {
    let scheduler = BestFitScheduler {
        nvme_preference: state.config.scheduling.nvme_preference,
        branch_affinity: true,
    };

    // Check if any queued job is a fit for this worker.
    let job_id_to_assign: Option<String> = {
        let job_registry = state.job_registry.read().await;
        let worker_registry = state.worker_registry.read().await;

        match worker_registry.get(worker_id) {
            Some(worker_state) => {
                let workers = vec![worker_state];
                let queued = job_registry.queued_jobs();

                queued.iter().find_map(|queued_job| {
                    // Only dispatch jobs that are either unassigned (general queue)
                    // or explicitly targeted at this worker (stage jobs).
                    let targeted_elsewhere = queued_job
                        .assigned_worker
                        .as_deref()
                        .map(|w| w != worker_id)
                        .unwrap_or(false);
                    if targeted_elsewhere {
                        return None;
                    }

                    // For jobs with an explicit worker assignment (submit_stage path),
                    // bypass the scheduler and dispatch directly.
                    let explicitly_targeted = queued_job
                        .assigned_worker
                        .as_deref()
                        .map(|w| w == worker_id)
                        .unwrap_or(false);

                    if explicitly_targeted
                        || scheduler.select_worker(queued_job, &workers).is_some()
                    {
                        Some(queued_job.job_id.clone())
                    } else {
                        None
                    }
                })
            }
            None => None,
        }
    };

    if let Some(_job_id) = job_id_to_assign {
        let mut job_registry_w = state.job_registry.write().await;
        if let Some(job) = job_registry_w.next_job_for(worker_id) {
            let assignment = build_job_assignment(job);
            if job_tx.send(Ok(assignment)).await.is_err() {
                return false;
            }
        }
    }

    true
}

/// Core logic for the `reserve_worker` RPC.
async fn do_reserve_worker(
    state: &Arc<ControllerState>,
    req: &ReserveWorkerRequest,
) -> Result<ReserveWorkerResponse, Status> {
    // Pick the first connected worker
    let worker_id = {
        let registry = state.worker_registry.read().await;
        let connected = registry.connected_workers();
        connected.first().map(|w| w.info.worker_id.clone())
    };

    let worker_id = match worker_id {
        Some(id) => id,
        None => {
            return Ok(ReserveWorkerResponse {
                job_group_id: String::new(),
                worker_id: String::new(),
                stages: Vec::new(),
                success: false,
                message: "No connected workers available".to_string(),
            });
        }
    };

    // Create a job group in-memory
    let mut group = ci_core::models::job_group::JobGroup::new(
        uuid::Uuid::new_v4(), // repo_id placeholder
        Some(req.branch.clone()).filter(|s| !s.is_empty()),
        Some(req.commit_sha.clone()).filter(|s| !s.is_empty()),
    );
    group.reserved_worker_id = Some(worker_id.clone());
    group.state = ci_core::models::job_group::JobGroupState::Reserved;
    group.updated_at = chrono::Utc::now();

    let group_id = group.id;

    // Build stage info from request stages
    let stage_infos: Vec<ci_core::proto::orchestrator::StageInfo> = req
        .stages
        .iter()
        .map(|name| ci_core::proto::orchestrator::StageInfo {
            stage_name: name.clone(),
            command: String::new(),
            required_cpu: 0,
            required_memory_mb: 0,
            required_disk_mb: 0,
            max_duration_secs: 0,
            parallel_group: String::new(),
            job_type: "common".to_string(),
        })
        .collect();

    // Add to job group registry
    {
        let mut jg_registry = state.job_group_registry.write().await;
        jg_registry.add_group(group);
    }

    // TODO: Persist job group to PostgreSQL via storage.rs
    // TODO: Reserve worker in Redis via ReservationManager for distributed locking

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
    let worker_id = {
        let jg_registry = state.job_group_registry.read().await;
        let group = jg_registry.get(&group_id).ok_or_else(|| {
            Status::not_found(format!("Job group {} not found", req.job_group_id))
        })?;
        group.reserved_worker_id.clone().ok_or_else(|| {
            Status::failed_precondition(format!(
                "Job group {} has no reserved worker",
                req.job_group_id
            ))
        })?
    };

    // Determine the job ID (use provided or generate)
    let job_id = if req.job_id.is_empty() {
        format!("{}-{}", group_id, req.stage_name)
    } else {
        req.job_id.clone()
    };

    // Determine command: use override if provided, otherwise use stage_name as placeholder
    let command = if req.command_override.is_empty() {
        format!("echo 'Running stage: {}'", req.stage_name)
    } else {
        req.command_override.clone()
    };

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
    job.assigned_worker = Some(worker_id.clone());
    job.environment = req.environment.clone();
    job.state = ci_core::models::job::JobState::Queued;

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
    }
    {
        let mut job_registry = state.job_registry.write().await;
        job_registry.add_job(job);
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
                environment: req.environment,
                cancel: None,
                job_group_id: group_id.to_string(),
                stage_name: req.stage_name.clone(),
                pre_script: String::new(),
                post_script: String::new(),
                max_duration_secs: 0,
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

    // TODO: Persist job to PostgreSQL via storage.rs

    Ok(SubmitStageResponse {
        job_id,
        stage_name: req.stage_name,
        accepted: true,
        message: "Stage submitted successfully".to_string(),
    })
}

// ---------------------------------------------------------------------------
// State & Service definitions
// ---------------------------------------------------------------------------

/// Shared controller state
pub struct ControllerState {
    pub config: ControllerConfig,
    /// Worker registry — shared with the HTTP sidecar via `Arc`.
    pub worker_registry: Arc<RwLock<WorkerRegistry>>,
    pub job_registry: RwLock<JobRegistry>,
    pub log_aggregator: RwLock<LogAggregator>,
    /// Job-group registry — shared with the HTTP sidecar via `Arc`.
    pub job_group_registry: Arc<RwLock<JobGroupRegistry>>,
    /// Channel to send job assignments (including cancel directives) to workers (worker_id -> sender)
    pub job_stream_senders: RwLock<
        std::collections::HashMap<String, tokio::sync::mpsc::Sender<Result<JobAssignment, Status>>>,
    >,
    /// Notify to wake the scheduler when a job is submitted or worker state changes
    pub scheduler_notify: Notify,
    /// Prometheus-compatible metrics — shared with the HTTP sidecar via `Clone`.
    pub metrics: Metrics,
}

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

        // Create job
        let mut job = Job::new(
            req.job_id.clone(),
            req.command.clone(),
            job_type,
            req.required_cpu,
            req.required_memory_mb,
            req.required_disk_mb,
        );
        job.isolation_required = req.isolation_required;
        job.branch_id = Some(req.branch_id).filter(|s| !s.is_empty());
        job.environment = req.environment;
        job.submitter_connection_id = Some(peer_addr);

        // Add to registry
        let mut registry = self.state.job_registry.write().await;
        registry.add_job(job);
        drop(registry);

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
            "CancelJob request: job_id={}, reason={}",
            req.job_id, req.reason
        );

        // Cancel the job and get the assigned worker
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
        let group = jg_registry.get(&group_id).ok_or_else(|| {
            Status::not_found(format!("Job group {} not found", req.job_group_id))
        })?;

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

        // TODO: Also check PostgreSQL for persisted state if not found in-memory

        Ok(Response::new(GetJobGroupStatusResponse {
            job_group_id: group_id.to_string(),
            state: group.state.to_string(),
            worker_id: group.reserved_worker_id.clone().unwrap_or_default(),
            stages: stage_statuses,
        }))
    }
}

/// Start the gRPC server.
///
/// Accepts the shared registries and metrics instance so the HTTP sidecar
/// (started in `main.rs`) can observe the same live data.
pub async fn run(
    config: ControllerConfig,
    worker_registry: Arc<RwLock<WorkerRegistry>>,
    job_group_registry: Arc<RwLock<JobGroupRegistry>>,
    metrics: Metrics,
) -> anyhow::Result<()> {
    let addr = config.bind_address.parse()?;

    let state = Arc::new(ControllerState {
        config: config.clone(),
        worker_registry,
        job_registry: RwLock::new(JobRegistry::new()),
        log_aggregator: RwLock::new(LogAggregator::new()),
        job_group_registry,
        job_stream_senders: RwLock::new(std::collections::HashMap::new()),
        scheduler_notify: Notify::new(),
        metrics,
    });

    // ── Heartbeat timeout detection background task ───────────────────────────
    {
        let state_for_hb = state.clone();
        let hb_timeout = config.workers.heartbeat_timeout_secs as u64;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(hb_timeout));
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

                    // Mark jobs as unknown for every dead worker
                    let mut job_registry = state_for_hb.job_registry.write().await;
                    for worker_id in &timed_out {
                        job_registry.mark_unknown_for_worker(worker_id);
                    }
                }
            }
        });
    }

    let service = OrchestratorService { state };
    // TODO: Add Tower auth interceptor when config.auth.enabled is true
    let server = OrchestratorServer::new(service);

    info!("Controller gRPC server listening on {}", addr);

    let mut server_builder = tonic::transport::Server::builder();

    // Configure TLS if enabled
    if let Some(ref tls) = config.tls {
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
