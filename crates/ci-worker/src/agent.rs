use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use ci_core::models::config::WorkerConfig;
use ci_core::proto::orchestrator::{
    DiskInfo, JobState, JobStatusUpdate, ReconnectRequest, RegisterRequest, RunningJobInfo,
};

use crate::executor::{ExecutionResult, Executor};
use crate::grpc_client::GrpcClient;
use crate::log_streamer::LogStreamer;
use crate::reconnect::ReconnectHandler;
use crate::stage_runner::{StageResult, StageRunner, StageState};

/// Timestamp captured once at agent startup. Used by the stage path
/// resolver to decide between the new `<scratch_root>/<gid>/...` layout
/// (groups created at-or-after this instant) and the legacy
/// `<log_dir>/<gid>/` + `<work_dir>/<gid>/<stage>/` layout
/// (pre-upgrade groups). See `retention-implementation-plan.md` §3.D.
pub static WORKER_STARTUP_TS: OnceLock<chrono::DateTime<chrono::Utc>> = OnceLock::new();

/// Read the worker startup timestamp captured by [`run`]. Returns `None`
/// before `run` initializes it (e.g. inside unit tests that don't boot
/// the agent), in which case path resolvers should fall back to the
/// legacy layout.
pub fn worker_startup_ts() -> Option<chrono::DateTime<chrono::Utc>> {
    WORKER_STARTUP_TS.get().copied()
}

/// Context for a job execution, grouping related parameters.
struct JobContext {
    worker_id: String,
    job_id: String,
    command: String,
    workspace_dir: String,
    stage_dir: Option<String>,
    /// Pre-resolved per-stage log path (new vs legacy layout decided up
    /// front in `handle_job_assignment`).
    log_path: String,
    pre_script: String,
    post_script: String,
    max_duration_secs: i32,
    job_group_id: String,
    stage_name: String,
    secret_values: Arc<Vec<String>>,
    environment: HashMap<String, String>,
    pre_script_lock: crate::stage_runner::ScriptLockConfig,
    post_script_lock: crate::stage_runner::ScriptLockConfig,
}

/// Outcome of a job or stage execution, used to build the final status report.
#[derive(Clone)]
struct StatusReport {
    state: JobState,
    message: String,
    exit_code: i32,
    phase: String,
    pre_exit_code: i32,
    post_exit_code: i32,
}

/// Main worker agent loop with reconnect support
pub async fn run(
    config: WorkerConfig,
    metrics: Option<crate::http_server::WorkerMetrics>,
) -> anyhow::Result<()> {
    info!("Worker agent starting");

    // Capture process-boot timestamp once. Used as the discriminator for
    // the new vs legacy on-disk layout (retention §3.D / §4). It's
    // idempotent — `set` returns an error if already initialized (e.g. a
    // test forced it earlier in the same process), which we ignore.
    let _ = WORKER_STARTUP_TS.set(chrono::Utc::now());

    tokio::fs::create_dir_all(&config.execution.work_dir).await?;
    tokio::fs::create_dir_all(&config.execution.log_dir).await?;
    tokio::fs::create_dir_all(&config.execution.repos_dir).await?;
    tokio::fs::create_dir_all(&config.execution.scratch_root).await?;

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
        match run_session(&config, &running_jobs, &reconnect_handler, &metrics).await {
            Ok(_) => {
                info!("Agent session ended gracefully");
                break;
            }
            Err(e) => {
                let err_msg = e.to_string();
                error!("Session error: {}", err_msg);

                // Fatal errors — don't retry, exit immediately
                let is_fatal = err_msg.contains("AlreadyExists")
                    || err_msg.contains("already has an active connection")
                    || err_msg.contains("Unauthenticated")
                    || err_msg.contains("Registration token required")
                    || err_msg.contains("Invalid worker token")
                    || err_msg.contains("Worker not registered")
                    || err_msg.contains("permission_denied");
                if is_fatal {
                    error!("Fatal error — not retrying. Fix the issue and restart.");
                    return Err(e);
                }

                warn!("Attempting to reconnect with exponential backoff...");

                let controller_addr = config.controller.address.clone();
                let reconnect_auth_token = config.token.clone();
                let reconnect_result = reconnect_handler
                    .reconnect(config.worker_id.clone(), |req| {
                        let addr = controller_addr.clone();
                        let token = reconnect_auth_token.clone();
                        async move {
                            let client =
                                GrpcClient::connect_with_options(&addr, None, token).await?;
                            client.reconnect(req).await
                        }
                    })
                    .await;

                match reconnect_result {
                    Ok(result) => {
                        let resume_directives =
                            ReconnectHandler::process_resume_directives(&result.response);
                        info!(
                            "Reconnected after {} attempt(s) in {:?}, received {} resume directive(s)",
                            result.attempts,
                            result.total_duration,
                            resume_directives.len()
                        );
                        handle_resume_directives(&running_jobs, &resume_directives).await;
                        // Continue to next loop iteration which will call run_session again
                    }
                    Err(e) => {
                        error!("Reconnect gave up after all attempts: {}", e);
                        running_jobs.write().await.clear();
                        return Err(e);
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
    reconnect_handler: &ReconnectHandler,
    metrics: &Option<crate::http_server::WorkerMetrics>,
) -> anyhow::Result<()> {
    let client = GrpcClient::connect_with_options(
        &config.controller.address,
        config.controller.tls.as_ref(),
        config.token.clone(),
    )
    .await?;

    // Check if we need to register or reconnect
    let jobs_snapshot = running_jobs.read().await.clone();
    if jobs_snapshot.is_empty() {
        // Fresh registration
        // Use real system info for totals, fall back to config. Narrow the
        // sysinfo refresh to cpu+memory ONLY — `System::new_all()` walks
        // every /proc/<pid> via __access_remote_vm and freezes the worker
        // (uninterruptible D-state) if any neighbour process is wedged.
        // We don't need the process list here, just totals. `with_memory`
        // needs an everything()-grade refresh kind so total_memory() is
        // actually populated (a nothing() kind leaves it at zero).
        let sys = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_cpu(sysinfo::CpuRefreshKind::nothing())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        );
        let real_cpu = sys.cpus().len() as u32;
        let real_memory_mb = sys.total_memory() / 1024 / 1024;
        let disk_details = collect_disk_details(&config.tracked_disk_paths);
        let total_disk_mb: u64 = disk_details.iter().map(|d| d.total_mb).sum();
        let total_disk_mb = if total_disk_mb > 0 {
            total_disk_mb
        } else {
            config.resources.total_disk_gb as u64 * 1024
        };

        let register_req = RegisterRequest {
            worker_id: config.worker_id.clone(),
            hostname: config.hostname.clone(),
            total_cpu: if real_cpu > 0 {
                real_cpu
            } else {
                config.resources.total_cpu
            },
            total_memory_mb: if real_memory_mb > 0 {
                real_memory_mb
            } else {
                config.resources.total_memory_gb as u64 * 1024
            },
            total_disk_mb,
            disk_type: config.resources.disk_type.clone(),
            supported_job_types: config.capabilities.supported_job_types.clone(),
            docker_enabled: config.capabilities.docker_enabled,
            labels: Vec::new(),
            disk_details,
            registration_token: {
                let t = config.token.as_deref().unwrap_or("");
                if t.starts_with("chola_reg_") {
                    t.to_string()
                } else {
                    String::new()
                }
            },
            worker_token: {
                let t = config.token.as_deref().unwrap_or("");
                if !t.starts_with("chola_reg_") {
                    t.to_string()
                } else {
                    String::new()
                }
            },
        };
        let resp = client.register(register_req).await?;
        info!("Registration response: {}", resp.message);

        // Report system metadata to controller REST API
        report_system_metadata(config).await;
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

    // Register any currently running jobs with the reconnect handler so it
    // has an up-to-date picture of worker state for the next reconnect attempt.
    let jobs_snapshot = running_jobs.read().await.clone();
    for job in &jobs_snapshot {
        reconnect_handler
            .register_job(job.job_id.clone(), job.log_offset)
            .await;
    }

    // Shared cancellation token: heartbeat task and main select loop both watch
    // this. When either stream fails the token is cancelled, causing a clean
    // shutdown of all tasks and triggering the outer reconnect loop.
    let cancel_token = CancellationToken::new();

    let (hb_tx, hb_rx) = mpsc::channel(32);
    let hb_interval = config.heartbeat.interval_secs;
    let worker_id = config.worker_id.clone();
    let running_jobs_for_hb = running_jobs.clone();
    let hb_token = cancel_token.clone();
    let tracked_paths = config.tracked_disk_paths.clone();

    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(hb_interval.into()));
        let mut sys = sysinfo::System::new();
        // Initial CPU refresh so the first reading isn't always 0
        sys.refresh_cpu_all();
        loop {
            tokio::select! {
                _ = hb_token.cancelled() => {
                    info!("Heartbeat task cancelled, stopping");
                    break;
                }
                _ = interval.tick() => {
                    let (cpu, mem, disk, load) = collect_system_metrics(&mut sys, &tracked_paths);
                    let disk_details = collect_disk_details(&tracked_paths);
                    let jobs = running_jobs_for_hb.read().await.clone();
                    let job_ids: Vec<String> = jobs.iter().map(|j| j.job_id.clone()).collect();
                    let msg = ci_core::proto::orchestrator::HeartbeatMessage {
                        worker_id: worker_id.clone(),
                        used_cpu_percent: cpu,
                        used_memory_mb: mem,
                        used_disk_mb: disk,
                        running_job_ids: job_ids,
                        system_load: load,
                        timestamp_unix: chrono::Utc::now().timestamp(),
                        disk_details,
                    };
                    if hb_tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut hb_stream = client.heartbeat(hb_rx).await?;
    let mut job_stream = client.job_stream(config.worker_id.clone()).await?;

    info!("Worker agent running");

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Session cancelled, shutting down");
                return Err(anyhow::anyhow!("Session cancelled"));
            }

            res = hb_stream.message() => {
                match res {
                    Ok(Some(ack)) => {
                        if !ack.ok {
                            warn!("Heartbeat rejected: {}", ack.message);
                        }
                    }
                    Ok(None) => {
                        warn!("Heartbeat stream closed by server");
                        cancel_token.cancel();
                        return Err(anyhow::anyhow!("Heartbeat stream closed"));
                    }
                    Err(e) => {
                        error!("Heartbeat stream error: {}", e);
                        cancel_token.cancel();
                        return Err(anyhow::anyhow!("Heartbeat error: {}", e));
                    }
                }
            }

            res = job_stream.message() => {
                match res {
                    Ok(Some(assignment)) => {
                        handle_job_assignment(
                            config, &client, &assignment,
                            running_jobs, metrics,
                        ).await;
                    }
                    Ok(None) => {
                        warn!("Job stream closed by server");
                        cancel_token.cancel();
                        return Err(anyhow::anyhow!("Job stream closed"));
                    }
                    Err(e) => {
                        error!("Job stream error: {}", e);
                        cancel_token.cancel();
                        return Err(anyhow::anyhow!("Job stream error: {}", e));
                    }
                }
            }
        }
    }
}

/// Handle a single job assignment message from the controller.
#[tracing::instrument(skip_all, fields(job_id = %assignment.job_id))]
async fn handle_job_assignment(
    config: &WorkerConfig,
    client: &GrpcClient,
    assignment: &ci_core::proto::orchestrator::JobAssignment,
    running_jobs: &Arc<tokio::sync::RwLock<Vec<RunningJobState>>>,
    metrics: &Option<crate::http_server::WorkerMetrics>,
) {
    let job_id = assignment.job_id.clone();

    // Check for purge directive (retention T1). When set, this
    // JobAssignment is purge-only — every other field is ignored. The
    // worker rm -rf's three per-group dirs (new scratch + legacy
    // log/work) best-effort and acks via report_job_status with
    // purge_ack=true. repos_dir is NEVER touched.
    if let Some(purge) = &assignment.purge_group_files {
        let gid = purge.group_id.clone();
        info!(group_id = %gid, "Received purge directive");

        let outcome = purge_group_dirs(
            &gid,
            Path::new(&config.execution.scratch_root),
            Path::new(&config.execution.log_dir),
            Path::new(&config.execution.work_dir),
        )
        .await;

        let (state, message) = match outcome {
            Ok(()) => (JobState::Success, String::new()),
            Err(msg) => (JobState::Failed, msg),
        };

        let ack = JobStatusUpdate {
            worker_id: config.worker_id.clone(),
            job_id: String::new(),
            state: state as i32,
            message,
            exit_code: 0,
            timestamp_unix: chrono::Utc::now().timestamp(),
            job_group_id: gid.clone(),
            stage_name: String::new(),
            phase: String::new(),
            pre_exit_code: 0,
            post_exit_code: 0,
            purge_ack: true,
        };
        if let Err(e) = client.report_job_status(ack).await {
            warn!(group_id = %gid, error = %e, "Failed to send purge_ack");
        }
        return;
    }

    // Check for cancel directive
    if let Some(cancel) = &assignment.cancel {
        info!(
            "Received cancel directive for job {}: {} (signal={})",
            job_id, cancel.reason, cancel.signal
        );

        let jobs = running_jobs.read().await;
        if let Some(job) = jobs.iter().find(|j| j.job_id == job_id) {
            if let Some(cancel_tx) = &job.cancel_tx {
                let signal = if cancel.signal > 0 { cancel.signal } else { 15 };
                let _ = cancel_tx.send(signal).await;
                info!("Sent cancel signal {} to job {}", signal, job_id);
            }
        } else {
            warn!("Received cancel for unknown job {}", job_id);
        }
        return;
    }

    info!(
        "Received job assignment: {} (command: {})",
        job_id, assignment.command
    );

    {
        let jobs = running_jobs.read().await;
        if jobs.iter().any(|j| j.job_id == job_id) {
            info!("Job {} already running, skipping", job_id);
            return;
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

    // Extract secret values: look up secret_env_keys in the environment map
    let secret_values: Vec<String> = assignment
        .secret_env_keys
        .iter()
        .filter_map(|key| assignment.environment.get(key))
        .filter(|v| !v.is_empty())
        .cloned()
        .collect();

    // Per-build workspace + log path. The new layout
    // (`<scratch_root>/<gid>/...`) kicks in when the group was created
    // at-or-after this worker process booted. The controller populates
    // `assignment.group_created_at` as RFC3339; empty string means
    // "unknown" → resolver falls through to legacy paths.
    let group_created_at = if assignment.group_created_at.is_empty() {
        None
    } else {
        chrono::DateTime::parse_from_rfc3339(&assignment.group_created_at)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    };
    let resolved = StageRunner::resolve_paths(
        &config.execution.scratch_root,
        &config.execution.log_dir,
        &config.execution.work_dir,
        group_created_at,
        worker_startup_ts(),
        &assignment.job_group_id,
        &assignment.stage_name,
        &job_id,
    );
    let workspace_dir = resolved.workspace_dir.to_string_lossy().to_string();
    let stages_dir = resolved
        .stages_dir
        .as_ref()
        .map(|path| path.to_string_lossy().to_string());
    let stage_dir = resolved
        .stage_dir
        .as_ref()
        .map(|path| path.to_string_lossy().to_string());
    let stage_log_path = resolved.log_path.to_string_lossy().to_string();

    // Build environment: assignment env + worker-local paths
    let mut environment: HashMap<String, String> =
        assignment.environment.clone().into_iter().collect();
    environment.insert("CHOLA_REPOS_DIR".into(), config.execution.repos_dir.clone());
    environment.insert("CHOLA_WORKSPACE_DIR".into(), workspace_dir.clone());
    environment.insert("CHOLA_WORK_DIR".into(), workspace_dir.clone());
    if let Some(ref stages_dir) = stages_dir {
        environment.insert("CHOLA_STAGES_DIR".into(), stages_dir.clone());
    }
    if !assignment.stage_name.is_empty() && assignment.stage_name != "__cleanup__" {
        environment.insert("CHOLA_STAGE_NAME".into(), assignment.stage_name.clone());
    }
    if let Some(ref stage_dir) = stage_dir {
        environment.insert("CHOLA_STAGE_DIR".into(), stage_dir.clone());
    }
    environment.insert("WORKER_ID".into(), config.worker_id.clone());

    let ctx = JobContext {
        worker_id: config.worker_id.clone(),
        job_id: job_id.clone(),
        command: assignment.command.clone(),
        workspace_dir,
        stage_dir,
        log_path: stage_log_path,
        pre_script: assignment.pre_script.clone(),
        post_script: assignment.post_script.clone(),
        max_duration_secs: assignment.max_duration_secs,
        job_group_id: assignment.job_group_id.clone(),
        stage_name: assignment.stage_name.clone(),
        secret_values: Arc::new(secret_values),
        environment,
        pre_script_lock: crate::stage_runner::ScriptLockConfig::from_proto(
            assignment.pre_script_lock.clone(),
        ),
        post_script_lock: crate::stage_runner::ScriptLockConfig::from_proto(
            assignment.post_script_lock.clone(),
        ),
    };

    let job_client = client.clone();
    let log_client = client.clone();
    let running_jobs_clone = running_jobs.clone();
    let metrics_clone = metrics.clone();

    info!("Spawning job {} with command: {}", ctx.job_id, ctx.command);

    tokio::spawn(async move {
        if let Some(ref m) = metrics_clone {
            m.active_jobs
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            m.jobs_executed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        info!(
            "Job {} task started, calling run_job_with_streaming",
            ctx.job_id
        );
        let result_state = run_job_with_streaming(
            &ctx,
            &job_client,
            &log_client,
            running_jobs_clone.clone(),
            cancel_rx,
        )
        .await;

        if let Some(ref m) = metrics_clone {
            m.active_jobs
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            match result_state {
                ci_core::proto::orchestrator::JobState::Success => {
                    m.jobs_succeeded
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                ci_core::proto::orchestrator::JobState::Cancelled => {
                    m.jobs_cancelled
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                _ => {
                    m.jobs_failed
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }

        let mut jobs = running_jobs_clone.write().await;
        jobs.retain(|j| j.job_id != job_id);
        info!("Job {} removed from running list", job_id);
    });
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

/// Send a job status update to the controller.
async fn report_status(
    client: &GrpcClient,
    ctx: &JobContext,
    report: StatusReport,
) -> anyhow::Result<ci_core::proto::orchestrator::JobStatusAck> {
    client
        .report_job_status(JobStatusUpdate {
            worker_id: ctx.worker_id.clone(),
            job_id: ctx.job_id.clone(),
            state: report.state as i32,
            message: report.message,
            exit_code: report.exit_code,
            timestamp_unix: chrono::Utc::now().timestamp(),
            job_group_id: ctx.job_group_id.clone(),
            stage_name: ctx.stage_name.clone(),
            phase: report.phase,
            pre_exit_code: report.pre_exit_code,
            post_exit_code: report.post_exit_code,
            purge_ack: false,
        })
        .await
}

/// Determine the final status report from a StageResult.
///
/// The command's exit code is the sole determinant of success/failure.
/// Post-script is cleanup — its exit code is reported for visibility but
/// MUST NOT change the job's success/failure determination.
fn determine_final_state_from_stage(result: &StageResult) -> StatusReport {
    let pre_exit_code = result.pre_exit_code.unwrap_or(0);
    let post_exit_code = result.post_exit_code.unwrap_or(0);

    // Phase indicates where the failure occurred (pre_script or command).
    // Post-script failures do NOT set the phase — they are cleanup.
    let phase = if result.pre_exit_code.is_some() && result.pre_exit_code != Some(0) {
        "pre_script".to_string()
    } else {
        "command".to_string()
    };

    // Map StageState -> JobState. The StageState was already determined by
    // command_exit_code (and pre_exit_code) in stage_runner — post_exit_code
    // never influences this.
    let (state, message) = match result.final_state {
        StageState::Success => {
            let mut msg = "Stage completed successfully".to_string();
            if post_exit_code != 0 {
                msg = format!(
                    "Stage completed successfully (post-script exited {})",
                    post_exit_code
                );
            }
            (JobState::Success, msg)
        }
        StageState::Failed => (
            JobState::Failed,
            format!(
                "Stage failed in phase: {} (exit code {})",
                phase, result.command_exit_code
            ),
        ),
        StageState::Cancelled => (
            JobState::Cancelled,
            "Stage cancelled or timed out".to_string(),
        ),
    };

    // exit_code is ALWAYS the command's exit code — never the post-script's.
    StatusReport {
        state,
        message,
        exit_code: result.command_exit_code,
        phase,
        pre_exit_code,
        post_exit_code,
    }
}

/// Determine the final status report from an executor result (legacy path).
fn determine_final_state_from_executor(
    result: &Result<ExecutionResult, anyhow::Error>,
) -> StatusReport {
    match result {
        Ok(r) if r.exit_code < 0 => {
            let signal = -r.exit_code;
            let signal_name = match signal {
                2 => "SIGINT",
                9 => "SIGKILL",
                15 => "SIGTERM",
                _ => "unknown signal",
            };
            StatusReport {
                state: JobState::Cancelled,
                exit_code: r.exit_code,
                message: format!("Job cancelled by {} signal", signal_name),
                phase: "command".to_string(),
                pre_exit_code: 0,
                post_exit_code: 0,
            }
        }
        Ok(r) if r.exit_code == 0 => StatusReport {
            state: JobState::Success,
            exit_code: 0,
            message: "Job completed successfully".to_string(),
            phase: "command".to_string(),
            pre_exit_code: 0,
            post_exit_code: 0,
        },
        Ok(r) => StatusReport {
            state: JobState::Failed,
            exit_code: r.exit_code,
            message: format!("Job failed with exit code {}", r.exit_code),
            phase: "command".to_string(),
            pre_exit_code: 0,
            post_exit_code: 0,
        },
        Err(e) => StatusReport {
            state: JobState::Failed,
            exit_code: -1,
            message: format!("Job execution error: {}", e),
            phase: "command".to_string(),
            pre_exit_code: 0,
            post_exit_code: 0,
        },
    }
}

/// Execute a single job with log streaming pipeline.
#[tracing::instrument(skip_all, fields(job_id = %ctx.job_id, stage = %ctx.stage_name))]
async fn run_job_with_streaming(
    ctx: &JobContext,
    job_client: &GrpcClient,
    log_client: &GrpcClient,
    running_jobs: Arc<tokio::sync::RwLock<Vec<RunningJobState>>>,
    cancel_rx: mpsc::Receiver<i32>,
) -> ci_core::proto::orchestrator::JobState {
    info!(
        "run_job_with_streaming: job_id={}, command={}, workspace_dir={}",
        ctx.job_id, ctx.command, ctx.workspace_dir
    );

    // Ensure the shared build workspace directory exists.
    if let Err(e) = tokio::fs::create_dir_all(&ctx.workspace_dir).await {
        warn!(
            "Failed to create workspace dir {}: {}",
            ctx.workspace_dir, e
        );
    }
    if let Some(ref stage_dir) = ctx.stage_dir {
        if let Err(e) = tokio::fs::create_dir_all(stage_dir).await {
            warn!("Failed to create stage dir {}: {}", stage_dir, e);
        }
    }

    let has_stage_scripts = !ctx.pre_script.is_empty() || !ctx.post_script.is_empty();

    // Report job started
    info!("Reporting job {} as Running to controller", ctx.job_id);
    let status_result = report_status(
        job_client,
        ctx,
        StatusReport {
            state: JobState::Running,
            message: "Job started".to_string(),
            exit_code: 0,
            phase: "command".to_string(),
            pre_exit_code: 0,
            post_exit_code: 0,
        },
    )
    .await;
    info!(
        "Reported job {} as Running: {:?}",
        ctx.job_id, status_result
    );

    // Set up: Executor -> mpsc -> LogStreamer -> gRPC StreamLogs -> Controller
    let (log_tx, log_rx) = mpsc::channel(256);

    // Log path was pre-resolved in `handle_job_assignment` (new vs
    // legacy layout). Just use it.
    let log_path = ctx.log_path.clone();

    info!(
        "Starting log streamer for job {} at {}",
        ctx.job_id, log_path
    );

    // Start log streamer in background
    let streamer = LogStreamer::new();
    let sw = ctx.worker_id.clone();
    let sj = ctx.job_id.clone();
    let slc = log_client.clone();
    let log_secrets = ctx.secret_values.clone();
    let log_handle =
        tokio::spawn(async move { streamer.stream(sw, sj, log_rx, slc, log_secrets).await });

    info!(
        "Log streamer spawned, now executing command for job {}",
        ctx.job_id
    );

    // Choose execution path: StageRunner (with pre/post) or direct Executor (legacy)
    let report = if has_stage_scripts || ctx.max_duration_secs > 0 {
        let stage_runner = StageRunner::new();
        let result = stage_runner
            .run_stage(
                &ctx.command,
                &ctx.pre_script,
                &ctx.post_script,
                &ctx.workspace_dir,
                &log_path,
                log_tx,
                cancel_rx,
                ctx.max_duration_secs,
                ctx.secret_values.clone(),
                &ctx.environment,
                &ctx.pre_script_lock,
                &ctx.post_script_lock,
            )
            .await;

        match result {
            Ok(stage_result) => determine_final_state_from_stage(&stage_result),
            Err(e) => StatusReport {
                state: JobState::Failed,
                exit_code: -1,
                message: format!("Stage execution error: {}", e),
                pre_exit_code: 0,
                post_exit_code: 0,
                phase: "command".to_string(),
            },
        }
    } else {
        let executor = Executor::new();
        let result = executor
            .execute_streaming(
                &ctx.command,
                &ctx.workspace_dir,
                &log_path,
                log_tx,
                cancel_rx,
                ctx.secret_values.clone(),
                &ctx.environment,
            )
            .await;
        determine_final_state_from_executor(&result)
    };

    info!(
        "Command execution finished for job {}: exit_code={}",
        ctx.job_id, report.exit_code
    );

    // Wait for log streamer to finish flushing
    let log_bytes = match log_handle.await {
        Ok(Ok(bytes)) => {
            info!("Log stream for job {} done: {} bytes", ctx.job_id, bytes);
            bytes
        }
        Ok(Err(e)) => {
            warn!("Log stream error for job {}: {}", ctx.job_id, e);
            0
        }
        Err(e) => {
            warn!("Log stream panic for job {}: {}", ctx.job_id, e);
            0
        }
    };

    // Update log offset in running state
    {
        let mut jobs = running_jobs.write().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.job_id == ctx.job_id) {
            job.log_offset = log_bytes;
        }
    }

    // Report completion (retry up to 3 times — exit code must reach controller)
    let final_message = format!("{} ({} bytes of logs)", report.message, log_bytes);
    let exit_code = report.exit_code;
    let final_state = report.state;
    let final_report = StatusReport {
        message: final_message,
        ..report
    };
    let mut reported = false;
    for attempt in 0..3u8 {
        match report_status(job_client, ctx, final_report.clone()).await {
            Ok(_) => {
                reported = true;
                break;
            }
            Err(e) => {
                warn!(
                    "report_status attempt {}/3 failed for job {}: {e}",
                    attempt + 1,
                    ctx.job_id
                );
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }
    if !reported {
        error!(
            "Failed to report final status for job {} after 3 attempts (exit_code={})",
            ctx.job_id, exit_code
        );
    }

    info!("Job {} completed with exit code {}", ctx.job_id, exit_code);

    final_state
}

/// Collect real CPU, memory, disk usage and system load via sysinfo.
///
/// Returns `(used_cpu_percent, used_memory_mb, used_disk_mb, system_load_1m)`.
fn collect_system_metrics(
    sys: &mut sysinfo::System,
    tracked_paths: &[String],
) -> (f64, u64, u64, f64) {
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpu = sys.global_cpu_usage() as f64;
    let used_memory_mb = sys.used_memory() / 1024 / 1024;

    let disk_details = collect_disk_details(tracked_paths);
    let used_disk_mb: u64 = disk_details.iter().map(|d| d.used_mb).sum();

    let load_avg = sysinfo::System::load_average();

    (cpu, used_memory_mb, used_disk_mb, load_avg.one)
}

/// Collect per-disk/partition info, filtering virtual filesystems.
fn collect_disk_details(tracked_paths: &[String]) -> Vec<DiskInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();

    // If specific paths configured, only show those
    if !tracked_paths.is_empty() {
        return disks
            .iter()
            .filter(|d| {
                let mount = d.mount_point().to_string_lossy();
                tracked_paths.iter().any(|p| mount.as_ref() == p.as_str())
            })
            .map(|d| DiskInfo {
                mount_point: d.mount_point().to_string_lossy().to_string(),
                device: d.name().to_string_lossy().to_string(),
                fs_type: d.file_system().to_string_lossy().to_string(),
                total_mb: d.total_space() / 1024 / 1024,
                used_mb: (d.total_space() - d.available_space()) / 1024 / 1024,
                available_mb: d.available_space() / 1024 / 1024,
            })
            .collect();
    }

    // Auto-detect: filter noise, dedup by device (keep shortest mount point per device)
    let mut by_device: std::collections::HashMap<String, DiskInfo> =
        std::collections::HashMap::new();
    for d in disks.iter() {
        let fs = d.file_system().to_string_lossy().to_string();
        let mount = d.mount_point().to_string_lossy().to_string();
        // Skip virtual/noise filesystems
        if matches!(
            fs.as_str(),
            "tmpfs" | "devtmpfs" | "efivarfs" | "squashfs" | "vfat"
        ) || mount.starts_with("/snap/")
            || mount.starts_with("/sys/")
            || mount.starts_with("/boot")
        {
            continue;
        }
        let device = d.name().to_string_lossy().to_string();
        let info = DiskInfo {
            mount_point: mount.clone(),
            device: device.clone(),
            fs_type: fs,
            total_mb: d.total_space() / 1024 / 1024,
            used_mb: (d.total_space() - d.available_space()) / 1024 / 1024,
            available_mb: d.available_space() / 1024 / 1024,
        };
        // Keep the shortest mount point per device (e.g., /data over /data/subanesh/...)
        by_device
            .entry(device)
            .and_modify(|existing| {
                if mount.len() < existing.mount_point.len() {
                    *existing = info.clone();
                }
            })
            .or_insert(info);
    }
    let mut result: Vec<DiskInfo> = by_device.into_values().collect();
    result.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    result
}

/// Derive the controller HTTP URL from config.
fn controller_http_url(config: &WorkerConfig) -> String {
    if let Some(url) = &config.controller.http_url {
        return url.clone();
    }
    // Replace gRPC port (typically 50051) with HTTP port (8080)
    let addr = &config.controller.address;
    if let Some(colon) = addr.rfind(':') {
        format!("{}:8080", &addr[..colon])
    } else {
        format!("{addr}:8080")
    }
}

/// Collect and POST system metadata to controller after registration.
async fn report_system_metadata(config: &WorkerConfig) {
    let sys = sysinfo::System::new_all();
    let metadata = serde_json::json!({
        "os_name": sysinfo::System::name().unwrap_or_default(),
        "os_version": sysinfo::System::os_version().unwrap_or_default(),
        "kernel_version": sysinfo::System::kernel_version().unwrap_or_default(),
        "arch": std::env::consts::ARCH,
        "host_name": sysinfo::System::host_name().unwrap_or_default(),
        "cpu_brand": sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default(),
        "cpu_count": sys.cpus().len(),
        "boot_time": sysinfo::System::boot_time(),
        "uptime": sysinfo::System::uptime(),
    });

    let base = controller_http_url(config);
    let url = format!("{base}/api/v1/workers/{}/metadata", config.worker_id);

    match reqwest::Client::new()
        .put(&url)
        .json(&metadata)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            info!("Reported system metadata to controller");
        }
        Ok(resp) => {
            warn!("Controller rejected metadata: {}", resp.status());
        }
        Err(e) => {
            warn!("Failed to send system metadata: {e}");
        }
    }
}

/// Best-effort `rm -rf` of the three per-group directories owned by this
/// worker for a given `group_id`. Returns `Ok(())` if every removal
/// either succeeded or returned `NotFound`; otherwise returns an
/// `Err(joined_error_messages)`.
///
/// Critically: only the new-layout scratch subtree, the legacy
/// `<log_dir>/<gid>/`, and the legacy `<work_dir>/<gid>/` are touched.
/// `repos_dir` is NEVER touched, and no path outside these three
/// per-group dirs is ever walked. Dependency-injected so the unit test
/// can exercise it against a tmpdir.
pub(crate) async fn purge_group_dirs(
    group_id: &str,
    scratch_root: &Path,
    log_dir: &Path,
    work_dir: &Path,
) -> Result<(), String> {
    let targets = [
        scratch_root.join(group_id),
        log_dir.join(group_id),
        work_dir.join(group_id),
    ];

    let mut failures: Vec<String> = Vec::new();
    for path in &targets {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => {
                info!(path = %path.display(), "Purged per-group dir");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Expected for groups that never ran on the new layout,
                // or that never wrote a workspace, etc.
            }
            Err(e) => {
                let msg = format!("{}: {}", path.display(), e);
                warn!(error = %msg, "Failed to purge per-group dir");
                failures.push(msg);
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms the narrowed sysinfo refresh still surfaces CPU count and
    /// total memory — same data the worker needs for RegisterRequest, but
    /// without the per-process walk that froze the worker on hosts with
    /// wedged D-state neighbours.
    #[test]
    fn narrowed_sysinfo_returns_cpu_and_memory_totals() {
        let sys = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_cpu(sysinfo::CpuRefreshKind::nothing())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        );
        assert!(sys.cpus().len() > 0, "cpus() must return at least one CPU");
        assert!(
            sys.total_memory() > 0,
            "total_memory() must report a non-zero value"
        );
    }

    /// Build a unique tmpdir for a test. We avoid the `tempfile` crate
    /// (not in the dep tree) and roll a simple uuid-named dir under
    /// `std::env::temp_dir()`.
    fn make_tmpdir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chola-purge-test-{}-{}",
            label,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create tmpdir");
        dir
    }

    /// Helper: seed a per-group directory with a file inside, so
    /// `remove_dir_all` has something to actually delete.
    fn seed_group_dir(root: &Path, gid: &str) {
        let dir = root.join(gid);
        std::fs::create_dir_all(&dir).expect("mkdir group dir");
        std::fs::write(dir.join("sentinel"), b"x").expect("write sentinel");
    }

    #[tokio::test]
    async fn purge_removes_new_and_legacy_layouts_keeps_repos() {
        let root = make_tmpdir("happy");
        let scratch = root.join("scratch");
        let log_dir = root.join("logs");
        let work_dir = root.join("work");
        let repos_dir = root.join("repos");
        let gid = "11111111-1111-1111-1111-111111111111";

        // New-layout: <scratch>/<gid>/{logs,workspace,artifacts}/
        let new_group = scratch.join(gid);
        std::fs::create_dir_all(new_group.join("logs")).unwrap();
        std::fs::create_dir_all(new_group.join("workspace/build")).unwrap();
        std::fs::create_dir_all(new_group.join("artifacts")).unwrap();
        std::fs::write(new_group.join("logs/build.log"), b"hello").unwrap();

        // Legacy layouts.
        seed_group_dir(&log_dir, gid);
        seed_group_dir(&work_dir, gid);

        // Sibling we must NOT touch — same name under repos_dir, plus an
        // unrelated subdir.
        seed_group_dir(&repos_dir, gid);
        seed_group_dir(&repos_dir, "foo");

        let res = purge_group_dirs(gid, &scratch, &log_dir, &work_dir).await;
        assert!(res.is_ok(), "purge failed: {:?}", res);

        // Three group dirs are gone.
        assert!(!scratch.join(gid).exists(), "scratch group dir still there");
        assert!(!log_dir.join(gid).exists(), "legacy log dir still there");
        assert!(!work_dir.join(gid).exists(), "legacy work dir still there");

        // repos_dir is untouched (both the same-gid and the sibling).
        assert!(repos_dir.join(gid).exists(), "repos_dir/<gid> was deleted");
        assert!(repos_dir.join("foo").exists(), "repos_dir/foo was deleted");

        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn purge_missing_paths_is_noop() {
        let root = make_tmpdir("missing");
        let scratch = root.join("scratch");
        let log_dir = root.join("logs");
        let work_dir = root.join("work");
        // Parent dirs exist, but no per-group subdir under any of them.
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::create_dir_all(&work_dir).unwrap();

        let gid = "22222222-2222-2222-2222-222222222222";
        let res = purge_group_dirs(gid, &scratch, &log_dir, &work_dir).await;
        assert!(res.is_ok(), "missing-path purge errored: {:?}", res);

        // Even fully-absent parent dirs (NotFound on the parent itself)
        // must be tolerated.
        let absent_root = root.join("does-not-exist");
        let res2 = purge_group_dirs(
            gid,
            &absent_root.join("scratch"),
            &absent_root.join("logs"),
            &absent_root.join("work"),
        )
        .await;
        assert!(res2.is_ok(), "absent-parent purge errored: {:?}", res2);

        std::fs::remove_dir_all(&root).ok();
    }
}
