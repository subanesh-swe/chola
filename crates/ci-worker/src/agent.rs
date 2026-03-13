use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use ci_core::models::config::WorkerConfig;
use ci_core::proto::orchestrator::{
    JobState, JobStatusUpdate, ReconnectRequest, RegisterRequest, RunningJobInfo,
};

use crate::executor::Executor;
use crate::grpc_client::GrpcClient;
use crate::log_streamer::LogStreamer;
use crate::reconnect::ReconnectHandler;

/// Main worker agent loop with reconnect support
pub async fn run(config: WorkerConfig) -> anyhow::Result<()> {
    info!("Worker agent starting");

    tokio::fs::create_dir_all(&config.execution.work_dir).await?;
    tokio::fs::create_dir_all(&config.execution.log_dir).await?;

    let reconnect_handler = ReconnectHandler::with_config(crate::reconnect::ReconnectConfig {
        initial_backoff: std::time::Duration::from_millis(config.reconnect.initial_delay_ms),
        max_backoff: std::time::Duration::from_millis(config.reconnect.max_delay_ms),
        multiplier: 1.5,
        max_attempts: config.reconnect.max_attempts,
    });

    let running_jobs: Arc<tokio::sync::RwLock<Vec<RunningJobState>>> =
        Arc::new(tokio::sync::RwLock::new(Vec::new()));

    // Main reconnect loop
    loop {
        match run_session(&config, &running_jobs, &reconnect_handler).await {
            Ok(_) => {
                info!("Agent session ended gracefully");
                break;
            }
            Err(e) => {
                error!("Session error: {}", e);
                warn!("Attempting to reconnect...");

                // Prepare reconnect state
                let jobs_snapshot = running_jobs.read().await.clone();
                let running_job_infos: Vec<RunningJobInfo> = jobs_snapshot
                    .iter()
                    .map(|j| RunningJobInfo {
                        job_id: j.job_id.clone(),
                        state: JobState::Running as i32,
                        log_offset: j.log_offset,
                    })
                    .collect();

                // Try to reconnect with backoff
                let client = GrpcClient::connect(&config.controller.address).await;
                match client {
                    Ok(client) => {
                        let reconnect_result = reconnect_handler
                            .reconnect(config.worker_id.clone(), |req| {
                                let client = client.clone();
                                async move { client.reconnect(req).await }
                            })
                            .await;

                        match reconnect_result {
                            Ok(result) => {
                                let resume_directives =
                                    ReconnectHandler::process_resume_directives(&result.response);
                                info!(
                                    "Reconnected successfully, received {} resume directives",
                                    resume_directives.len()
                                );
                                handle_resume_directives(&running_jobs, &resume_directives).await;
                            }
                            Err(e) => {
                                error!("Reconnect failed: {}", e);
                                // Clear running jobs state since controller doesn't know about them
                                running_jobs.write().await.clear();
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect to controller: {}", e);
                        running_jobs.write().await.clear();
                    }
                }
            }
        }
    }

    Ok(())
}

/// Run a single session (until disconnect)
async fn run_session(
    config: &WorkerConfig,
    running_jobs: &Arc<tokio::sync::RwLock<Vec<RunningJobState>>>,
    _reconnect_handler: &ReconnectHandler,
) -> anyhow::Result<()> {
    let client = GrpcClient::connect(&config.controller.address).await?;

    // Check if we need to register or reconnect
    let jobs_snapshot = running_jobs.read().await.clone();
    if jobs_snapshot.is_empty() {
        // Fresh registration
        let register_req = RegisterRequest {
            worker_id: config.worker_id.clone(),
            hostname: config.hostname.clone(),
            total_cpu: config.resources.total_cpu,
            total_memory_mb: config.resources.total_memory_gb * 1024,
            total_disk_mb: config.resources.total_disk_gb * 1024,
            disk_type: config.resources.disk_type.clone(),
            supported_job_types: config.capabilities.supported_job_types.clone(),
            docker_enabled: config.capabilities.docker_enabled,
        };
        let resp = client.register(register_req).await?;
        info!("Registration response: {}", resp.message);
    } else {
        // Attempt reconnect with current state
        let running_job_infos: Vec<RunningJobInfo> = jobs_snapshot
            .iter()
            .map(|j| RunningJobInfo {
                job_id: j.job_id.clone(),
                state: JobState::Running as i32,
                log_offset: j.log_offset,
            })
            .collect();

        let reconnect_req = ReconnectRequest {
            worker_id: config.worker_id.clone(),
            running_jobs: running_job_infos,
        };
        let resp = client.reconnect(reconnect_req).await?;
        if !resp.accepted {
            return Err(anyhow::anyhow!("Reconnect rejected: {}", resp.message));
        }
        info!("Reconnect accepted: {}", resp.message);
        // Convert LogResumeDirective to (String, u64) tuples
        let resume_directives: Vec<(String, u64)> = resp
            .log_resumes
            .iter()
            .map(|d| (d.job_id.clone(), d.resume_from_offset))
            .collect();
        handle_resume_directives(running_jobs, &resume_directives).await;
    }

    let (hb_tx, hb_rx) = mpsc::channel(32);
    let hb_interval = config.heartbeat.interval_secs;
    let worker_id = config.worker_id.clone();
    let running_jobs_for_hb = running_jobs.clone();

    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(hb_interval.into()));
        loop {
            interval.tick().await;
            let jobs = running_jobs_for_hb.read().await.clone();
            let job_ids: Vec<String> = jobs.iter().map(|j| j.job_id.clone()).collect();
            let msg = ci_core::proto::orchestrator::HeartbeatMessage {
                worker_id: worker_id.clone(),
                used_cpu_percent: 0.0,
                used_memory_mb: 0,
                used_disk_mb: 0,
                running_job_ids: job_ids,
                system_load: 0.0,
                timestamp_unix: chrono::Utc::now().timestamp(),
            };
            if hb_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut hb_stream = client.heartbeat(hb_rx).await?;
    let mut job_stream = client.job_stream(config.worker_id.clone()).await?;

    info!("Worker agent running");

    loop {
        tokio::select! {
            res = hb_stream.message() => {
                match res {
                    Ok(Some(ack)) => {
                        if !ack.ok {
                            warn!("Heartbeat rejected: {}", ack.message);
                        }
                    }
                    Ok(None) => {
                        warn!("Heartbeat stream closed by server");
                        return Err(anyhow::anyhow!("Heartbeat stream closed"));
                    }
                    Err(e) => {
                        error!("Heartbeat stream error: {}", e);
                        return Err(anyhow::anyhow!("Heartbeat error: {}", e));
                    }
                }
            }

            res = job_stream.message() => {
                match res {
                    Ok(Some(assignment)) => {
                        let job_id = assignment.job_id.clone();

                        // Check for cancel directive
                        if let Some(cancel) = &assignment.cancel {
                            info!("Received cancel directive for job {}: {} (signal={})",
                                  job_id, cancel.reason, cancel.signal);

                            // Find the job and send cancel signal
                            let jobs = running_jobs.read().await;
                            if let Some(job) = jobs.iter().find(|j| j.job_id == job_id) {
                                if let Some(cancel_tx) = &job.cancel_tx {
                                    // Send the signal number to use for killing the process
                                    let signal = if cancel.signal > 0 { cancel.signal } else { 15 }; // default to SIGTERM
                                    let _ = cancel_tx.send(signal).await;
                                    info!("Sent cancel signal {} to job {}", signal, job_id);
                                }
                            } else {
                                warn!("Received cancel for unknown job {}", job_id);
                            }
                            continue;
                        }

                        info!("Received job assignment: {}", job_id);

                        {
                            let jobs = running_jobs.read().await;
                            if jobs.iter().any(|j| j.job_id == job_id) {
                                info!("Job {} already running, skipping", job_id);
                                continue;
                            }
                        }

                        // Create cancel channel
                        let (cancel_tx, cancel_rx) = mpsc::channel(1);

                        // Add to running jobs
                        {
                            let mut jobs = running_jobs.write().await;
                            jobs.push(RunningJobState {
                                job_id: job_id.clone(),
                                log_offset: 0,
                                cancel_tx: Some(cancel_tx),
                            });
                        }

                        let work_dir = config.execution.work_dir.clone();
                        let log_dir = config.execution.log_dir.clone();
                        let job_client = client.clone();
                        let log_client = client.clone();
                        let worker_id_clone = config.worker_id.clone();
                        let running_jobs_clone = running_jobs.clone();

                        tokio::spawn(async move {
                            run_job_with_streaming(
                                &worker_id_clone, &job_id, &assignment.command,
                                &work_dir, &log_dir, &job_client, &log_client,
                                running_jobs_clone.clone(), cancel_rx,
                            ).await;

                            let mut jobs = running_jobs_clone.write().await;
                            jobs.retain(|j| j.job_id != job_id);
                            info!("Job {} removed from running list", job_id);
                        });
                    }
                    Ok(None) => {
                        warn!("Job stream closed by server");
                        return Err(anyhow::anyhow!("Job stream closed"));
                    }
                    Err(e) => {
                        error!("Job stream error: {}", e);
                        return Err(anyhow::anyhow!("Job stream error: {}", e));
                    }
                }
            }
        }
    }
}

/// Handle resume directives from controller after reconnect
async fn handle_resume_directives(
    running_jobs: &Arc<tokio::sync::RwLock<Vec<RunningJobState>>>,
    directives: &[(String, u64)],
) {
    if directives.is_empty() {
        return;
    }

    let mut jobs = running_jobs.write().await;
    for (job_id, resume_from_offset) in directives {
        if let Some(job) = jobs.iter_mut().find(|j| &j.job_id == job_id) {
            job.log_offset = *resume_from_offset;
            info!(
                "Job {} will resume log streaming from offset {}",
                job_id, resume_from_offset
            );
        }
    }
}

/// Running job state for tracking during reconnect
#[derive(Clone, Debug)]
struct RunningJobState {
    job_id: String,
    log_offset: u64,
    /// Channel to signal job cancellation (sends the signal number to use)
    cancel_tx: Option<mpsc::Sender<i32>>,
}

/// Execute a single job with log streaming pipeline.
async fn run_job_with_streaming(
    worker_id: &str,
    job_id: &str,
    command: &str,
    work_dir: &str,
    log_dir: &str,
    job_client: &GrpcClient,
    log_client: &GrpcClient,
    running_jobs: Arc<tokio::sync::RwLock<Vec<RunningJobState>>>,
    cancel_rx: mpsc::Receiver<i32>,
) {
    // Report job started
    let _ = job_client
        .report_job_status(JobStatusUpdate {
            worker_id: worker_id.to_string(),
            job_id: job_id.to_string(),
            state: JobState::Running as i32,
            message: "Job started".to_string(),
            exit_code: 0,
            timestamp_unix: chrono::Utc::now().timestamp(),
            output: String::new(),
        })
        .await;

    // Set up: Executor → mpsc → LogStreamer → gRPC StreamLogs → Controller
    let (log_tx, log_rx) = mpsc::channel(256);
    let log_path = format!("{}/{}.log", log_dir, job_id);

    // Start log streamer in background
    let streamer = LogStreamer::new();
    let sw = worker_id.to_string();
    let sj = job_id.to_string();
    let slc = log_client.clone();
    let log_handle = tokio::spawn(async move { streamer.stream(sw, sj, log_rx, slc).await });

    // Execute — lines flow through log_tx to streamer
    let executor = Executor::new();
    let result = executor
        .execute_streaming(command, work_dir, &log_path, log_tx, cancel_rx)
        .await;

    // Wait for log streamer to finish flushing
    let log_bytes = match log_handle.await {
        Ok(Ok(bytes)) => {
            info!("Log stream for job {} done: {} bytes", job_id, bytes);
            bytes
        }
        Ok(Err(e)) => {
            warn!("Log stream error for job {}: {}", job_id, e);
            0
        }
        Err(e) => {
            warn!("Log stream panic for job {}: {}", job_id, e);
            0
        }
    };

    // Determine final state
    // Negative exit code indicates cancellation by signal: -2=SIGINT, -9=SIGKILL, -15=SIGTERM
    let (state, exit_code, message) = match &result {
        Ok(r) if r.exit_code < 0 => {
            let signal = -r.exit_code;
            let signal_name = match signal {
                2 => "SIGINT",
                9 => "SIGKILL",
                15 => "SIGTERM",
                _ => "unknown signal",
            };
            (
                JobState::Cancelled,
                r.exit_code,
                format!(
                    "Job cancelled by {} signal ({} bytes of logs)",
                    signal_name, log_bytes
                ),
            )
        }
        Ok(r) if r.exit_code == 0 => (
            JobState::Success,
            0,
            format!("Job completed successfully ({} bytes of logs)", log_bytes),
        ),
        Ok(r) => (
            JobState::Failed,
            r.exit_code,
            format!(
                "Job failed with exit code {} ({} bytes of logs)",
                r.exit_code, log_bytes
            ),
        ),
        Err(e) => (JobState::Failed, -1, format!("Job execution error: {}", e)),
    };

    // Update log offset in running state
    {
        let mut jobs = running_jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.job_id == job_id) {
            job.log_offset = log_bytes;
        }
    }

    // Report completion
    let _ = job_client
        .report_job_status(JobStatusUpdate {
            worker_id: worker_id.to_string(),
            job_id: job_id.to_string(),
            state: state as i32,
            message,
            exit_code,
            timestamp_unix: chrono::Utc::now().timestamp(),
            output: String::new(), // output is in the log stream now
        })
        .await;

    info!("Job {} completed with exit code {}", job_id, exit_code);
}
