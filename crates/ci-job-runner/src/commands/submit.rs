use std::io::Write;

use ci_core::proto::orchestrator::{
    CancelJobRequest, GetJobStatusRequest, GetJobStatusResponse, JobState, SubmitJobRequest,
    WatchJobLogsRequest,
};
use tracing::{info, warn};

pub async fn execute(
    client: &mut super::Client,
    job_id: String,
    job_type: String,
    command_parts: Vec<String>,
) -> anyhow::Result<()> {
    let command = command_parts.join(" ");

    info!("Submitting job: {} command: {}", job_id, command);

    let request = tonic::Request::new(SubmitJobRequest {
        job_id: job_id.clone(),
        command: command.clone(),
        job_type,
        required_cpu: 1,
        required_memory_mb: 1024,
        required_disk_mb: 1024,
        isolation_required: false,
        branch_id: String::new(),
        environment: std::collections::HashMap::new(),
    });

    let response = client.submit_job(request).await?;
    let resp = response.into_inner();

    if !resp.accepted {
        return Err(anyhow::anyhow!("Job rejected: {}", resp.message));
    }

    info!("Job accepted: {} - {}", resp.job_id, resp.message);
    info!("Streaming logs... (Ctrl+C to cancel)");
    println!("{}", "-".repeat(60));

    let log_request = tonic::Request::new(WatchJobLogsRequest {
        job_id: job_id.clone(),
        from_offset: 0,
        job_group_id: String::new(),
        stage_name: String::new(),
    });

    match client.watch_job_logs(log_request).await {
        Ok(log_response) => {
            let mut stream = log_response.into_inner();
            let jid = job_id.clone();

            tokio::select! {
                result = stream_logs(&mut stream) => {
                    println!("{}", "-".repeat(60));
                    match result {
                        Ok(()) => exit_with_job_status(client, &jid).await,
                        Err(e) => {
                            warn!("Log stream broke: {}", e);
                            info!("Checking final job status...");
                            exit_with_job_status(client, &jid).await
                        }
                    }
                }

                _ = tokio::signal::ctrl_c() => {
                    println!("\n{}", "-".repeat(60));
                    handle_cancel(client, &jid).await
                }
            }
        }
        Err(e) => {
            warn!("Failed to watch job logs: {}", e);
            info!("Falling back to polling for status...");
            fallback_poll_status(client, &job_id).await
        }
    }
}

/// Stream log chunks to stdout until the stream ends.
pub async fn stream_logs(
    stream: &mut tonic::Streaming<ci_core::proto::orchestrator::LogChunk>,
) -> anyhow::Result<()> {
    while let Some(chunk) = stream.message().await? {
        if !chunk.data.is_empty() {
            let text = String::from_utf8_lossy(&chunk.data);
            print!("{}", text);
            std::io::stdout().flush().ok();
        }
    }
    Ok(())
}

/// Query the controller for the final job status and exit with its exit code.
/// If the job is not yet terminal, polls until it reaches a terminal state
/// (with a 5-minute timeout). Retries up to 3 times on transport errors.
pub async fn exit_with_job_status(client: &mut super::Client, job_id: &str) -> anyhow::Result<()> {
    // Try up to 3 times (handles transient transport errors after stream close)
    let mut last_err = String::new();
    for attempt in 0..3u8 {
        let request = tonic::Request::new(GetJobStatusRequest {
            job_id: job_id.to_string(),
        });
        match client.get_job_status(request).await {
            Ok(resp) => return process_terminal_status(client, job_id, resp.into_inner()).await,
            Err(e) => {
                last_err = e.to_string();
                if attempt < 2 {
                    warn!(
                        "get_job_status attempt {}/3 failed: {}, retrying...",
                        attempt + 1,
                        e
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }
    eprintln!(
        "Job {} status unknown after 3 retries: {}",
        job_id, last_err
    );
    std::process::exit(1)
}

/// Process a job status response: exit with the correct code based on state.
/// If not yet terminal, polls until terminal or timeout.
async fn process_terminal_status(
    client: &mut super::Client,
    job_id: &str,
    status: GetJobStatusResponse,
) -> anyhow::Result<()> {
    let state = JobState::try_from(status.state).unwrap_or(JobState::Unknown);
    match state {
        JobState::Success => {
            info!("Job {} completed successfully", job_id);
            Ok(())
        }
        JobState::Failed => {
            let code = if status.exit_code == 0 {
                1
            } else {
                status.exit_code
            };
            eprintln!(
                "Job {} failed (exit code: {}): {}",
                job_id, code, status.message
            );
            std::process::exit(code)
        }
        JobState::Cancelled => {
            eprintln!("Job {} was cancelled", job_id);
            std::process::exit(130)
        }
        _ => {
            warn!(
                "Job {} not yet terminal (state: {:?}), waiting...",
                job_id, state
            );
            match wait_for_job_termination_with_timeout(client, job_id, 300).await {
                Ok(final_status) => {
                    let final_state =
                        JobState::try_from(final_status.state).unwrap_or(JobState::Unknown);
                    match final_state {
                        JobState::Success => {
                            info!("Job {} completed successfully", job_id);
                            Ok(())
                        }
                        JobState::Failed => {
                            let code = if final_status.exit_code == 0 {
                                1
                            } else {
                                final_status.exit_code
                            };
                            eprintln!(
                                "Job {} failed (exit code: {}): {}",
                                job_id, code, final_status.message
                            );
                            std::process::exit(code)
                        }
                        JobState::Cancelled => {
                            eprintln!("Job {} was cancelled", job_id);
                            std::process::exit(130)
                        }
                        _ => {
                            eprintln!(
                                "Job {} stuck in state {:?} after timeout",
                                job_id, final_state
                            );
                            std::process::exit(1)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to determine job {} final status: {}", job_id, e);
                    std::process::exit(1)
                }
            }
        }
    }
}

/// Handle Ctrl+C: cancel the job on the controller and wait for termination.
async fn handle_cancel(client: &mut super::Client, job_id: &str) -> anyhow::Result<()> {
    info!("Received Ctrl+C, cancelling job {}...", job_id);

    let cancel = tonic::Request::new(CancelJobRequest {
        job_id: job_id.to_string(),
        reason: "User interrupted (Ctrl+C)".to_string(),
        job_group_id: String::new(),
    });

    match client.cancel_job(cancel).await {
        Ok(cr) => {
            let cr = cr.into_inner();
            if cr.accepted {
                info!("Cancellation accepted: {}", cr.message);
                match wait_for_job_termination(client, job_id).await {
                    Ok(s) => info!(
                        "Job {} terminated with state: {:?}",
                        job_id,
                        JobState::try_from(s.state).unwrap_or(JobState::Unknown)
                    ),
                    Err(e) => warn!("Error waiting for termination: {}", e),
                }
            } else {
                warn!("Cancel not accepted: {}", cr.message);
            }
        }
        Err(e) => warn!("Failed to cancel job: {}", e),
    }

    Err(anyhow::anyhow!("Job cancelled by user"))
}

/// Wait for job to reach terminal state. Returns the full status response.
/// Uses a default 30-minute timeout.
pub async fn wait_for_job_termination(
    client: &mut super::Client,
    job_id: &str,
) -> anyhow::Result<GetJobStatusResponse> {
    wait_for_job_termination_with_timeout(client, job_id, 1800).await
}

/// Wait for job to reach terminal state with an explicit timeout in seconds.
/// Returns the full `GetJobStatusResponse` on success.
pub async fn wait_for_job_termination_with_timeout(
    client: &mut super::Client,
    job_id: &str,
    timeout_secs: u64,
) -> anyhow::Result<GetJobStatusResponse> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(anyhow::anyhow!(
                "Timeout waiting for job {} to terminate after {}s",
                job_id,
                timeout_secs
            ));
        }

        interval.tick().await;

        let request = tonic::Request::new(GetJobStatusRequest {
            job_id: job_id.to_string(),
        });

        match client.get_job_status(request).await {
            Ok(response) => {
                let status = response.into_inner();

                if !status.found {
                    return Err(anyhow::anyhow!("Job not found on controller"));
                }

                let state = JobState::try_from(status.state).unwrap_or(JobState::Unknown);

                match state {
                    JobState::Success | JobState::Failed | JobState::Cancelled => {
                        return Ok(status)
                    }
                    _ => continue,
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query job status: {}", e));
            }
        }
    }
}

/// Fallback to polling for job status if WatchJobLogs fails.
/// Times out after 30 minutes.
pub async fn fallback_poll_status(client: &mut super::Client, job_id: &str) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(1800);

    loop {
        if tokio::time::Instant::now() > deadline {
            eprintln!("Timeout waiting for job {} status", job_id);
            std::process::exit(1);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let request = tonic::Request::new(GetJobStatusRequest {
            job_id: job_id.to_string(),
        });

        match client.get_job_status(request).await {
            Ok(response) => {
                let status = response.into_inner();

                if !status.found {
                    return Err(anyhow::anyhow!("Job not found on controller"));
                }

                let state = JobState::try_from(status.state).unwrap_or(JobState::Unknown);

                match state {
                    JobState::Success => {
                        println!("{}", "-".repeat(60));
                        if !status.output.is_empty() {
                            println!("{}", status.output);
                            println!("{}", "-".repeat(60));
                        }
                        info!(
                            "Job {} completed successfully (exit code: {})",
                            status.job_id, status.exit_code
                        );
                        return Ok(());
                    }
                    JobState::Failed => {
                        println!("{}", "-".repeat(60));
                        if !status.output.is_empty() {
                            eprintln!("{}", status.output);
                            println!("{}", "-".repeat(60));
                        }
                        let code = if status.exit_code == 0 {
                            1
                        } else {
                            status.exit_code
                        };
                        eprintln!(
                            "Job {} failed (exit code: {}): {}",
                            status.job_id, code, status.message
                        );
                        std::process::exit(code)
                    }
                    JobState::Cancelled => {
                        eprintln!("Job {} was cancelled", status.job_id);
                        std::process::exit(130)
                    }
                    _ => continue,
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query job status: {}", e));
            }
        }
    }
}
