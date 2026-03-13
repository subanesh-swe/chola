use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use ci_core::models::config::ControllerConfig;
use ci_core::models::job::{Job, JobType};
use ci_core::proto::orchestrator::{
    orchestrator_server::{Orchestrator, OrchestratorServer},
    CancelDirective, CancelJobRequest, CancelJobResponse, GetJobStatusRequest,
    GetJobStatusResponse, HeartbeatAck, HeartbeatMessage, JobAssignment, JobStatusAck,
    JobStatusUpdate, JobStreamRequest, LogAck, LogChunk, LogResumeDirective, ReconnectRequest,
    ReconnectResponse, RegisterRequest, RegisterResponse, SubmitJobRequest, SubmitJobResponse,
    WatchJobLogsRequest,
};

use crate::job_registry::JobRegistry;
use crate::log_aggregator::LogAggregator;
use crate::worker_registry::WorkerRegistry;

/// Shared controller state
pub struct ControllerState {
    pub config: ControllerConfig,
    pub worker_registry: RwLock<WorkerRegistry>,
    pub job_registry: RwLock<JobRegistry>,
    pub log_aggregator: RwLock<LogAggregator>,
    /// Channel to send job assignments (including cancel directives) to workers (worker_id -> sender)
    pub job_stream_senders: RwLock<
        std::collections::HashMap<String, tokio::sync::mpsc::Sender<Result<JobAssignment, Status>>>,
    >,
}

/// gRPC service implementation
pub struct OrchestratorService {
    state: Arc<ControllerState>,
}

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
            // Job assignment loop
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            loop {
                interval.tick().await;

                // Check if channel is still open
                if job_tx.is_closed() {
                    info!("Job stream channel closed for worker {}", worker_id);
                    break;
                }

                let mut registry = state.job_registry.write().await;
                if let Some(job) = registry.next_job_for(&worker_id) {
                    let assignment = JobAssignment {
                        job_id: job.job_id,
                        command: job.command,
                        job_type: job.job_type.to_string(),
                        required_cpu: job.required_cpu,
                        required_memory_mb: job.required_memory_mb,
                        required_disk_mb: job.required_disk_mb,
                        isolation_required: job.isolation_required,
                        branch_id: job.branch_id.unwrap_or_default(),
                        environment: job.environment,
                        cancel: None, // No cancel directive for normal job assignment
                    };
                    if job_tx.send(Ok(assignment)).await.is_err() {
                        break;
                    }
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
                let state = match job.state {
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
                };

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
                    // Send a JobAssignment with cancel directive set
                    let cancel_assignment = JobAssignment {
                        job_id: req.job_id.clone(),
                        command: String::new(),
                        job_type: String::new(),
                        required_cpu: 0,
                        required_memory_mb: 0,
                        required_disk_mb: 0,
                        isolation_required: false,
                        branch_id: String::new(),
                        environment: std::collections::HashMap::new(),
                        cancel: Some(CancelDirective {
                            job_id: req.job_id.clone(),
                            reason: req.reason.clone(),
                            signal: 2, // SIGINT - user pressed Ctrl+C
                        }),
                    };
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
}

/// Start the gRPC server
pub async fn run(config: ControllerConfig) -> anyhow::Result<()> {
    let addr = config.bind_address.parse()?;

    let state = Arc::new(ControllerState {
        config: config.clone(),
        worker_registry: RwLock::new(WorkerRegistry::new()),
        job_registry: RwLock::new(JobRegistry::new()),
        log_aggregator: RwLock::new(LogAggregator::new()),
        job_stream_senders: RwLock::new(std::collections::HashMap::new()),
    });

    let service = OrchestratorService { state };
    let server = OrchestratorServer::new(service);

    info!("Controller gRPC server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(server)
        .serve(addr)
        .await?;

    Ok(())
}
