use std::io::Write;

use ci_core::proto::orchestrator::{
    orchestrator_client::OrchestratorClient, CancelJobRequest, GetJobStatusRequest, JobState,
    SubmitJobRequest, WatchJobLogsRequest,
};
use tonic::transport::Channel;
use tracing::{info, warn};

pub async fn execute(
    client: &mut OrchestratorClient<Channel>,
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
                result = async {
                    while let Some(chunk) = stream.message().await? {
                        if !chunk.data.is_empty() {
                            let text = String::from_utf8_lossy(&chunk.data);
                            print!("{}", text);
                            std::io::stdout().flush().ok();
                        }
                    }
                    Ok::<(), anyhow::Error>(())
                } => {
                    match result {
                        Ok(()) => {
                            println!("{}", "-".repeat(60));
                            info!("Job {} completed", jid);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }

                _ = tokio::signal::ctrl_c() => {
                    println!("\n{}", "-".repeat(60));
                    info!("Received Ctrl+C, cancelling job {}...", jid);

                    let cancel = tonic::Request::new(CancelJobRequest {
                        job_id: jid.clone(),
                        reason: "User interrupted (Ctrl+C)".to_string(),
                        job_group_id: String::new(),
                    });

                    match client.cancel_job(cancel).await {
                        Ok(cr) => {
                            let cr = cr.into_inner();
                            if cr.accepted {
                                info!("Cancellation accepted: {}", cr.message);
                                let state = wait_for_job_termination(client, &jid).await;
                                match state {
                                    Ok(s) => info!("Job {} terminated with state: {}", jid, s),
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
            }
        }
        Err(e) => {
            warn!("Failed to watch job logs: {}", e);
            info!("Falling back to polling for status...");
            fallback_poll_status(client, &job_id).await
        }
    }
}

/// Wait for job to reach terminal state (SUCCESS, FAILED, or CANCELLED)
pub async fn wait_for_job_termination(
    client: &mut OrchestratorClient<Channel>,
    job_id: &str,
) -> anyhow::Result<String> {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
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
                    JobState::Success => return Ok("SUCCESS".to_string()),
                    JobState::Failed => return Ok("FAILED".to_string()),
                    JobState::Cancelled => return Ok("CANCELLED".to_string()),
                    _ => continue,
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query job status: {}", e));
            }
        }
    }
}

/// Fallback to polling for job status if WatchJobLogs fails
pub async fn fallback_poll_status(
    client: &mut OrchestratorClient<Channel>,
    job_id: &str,
) -> anyhow::Result<()> {
    loop {
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
                        return Err(anyhow::anyhow!(
                            "Job {} failed (exit code: {}): {}",
                            status.job_id,
                            status.exit_code,
                            status.message
                        ));
                    }
                    JobState::Cancelled => {
                        return Err(anyhow::anyhow!("Job {} was cancelled", status.job_id));
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
