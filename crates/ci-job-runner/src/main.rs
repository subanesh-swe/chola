use clap::Parser;
use tonic::transport::Channel;
use tracing::{info, warn};

use ci_core::models::config::ControllerConfig;
use ci_core::proto::orchestrator::{
    orchestrator_client::OrchestratorClient, CancelJobRequest, SubmitJobRequest,
    WatchJobLogsRequest,
};

/// CI Job Runner - Submit jobs to CI Controller
#[derive(Parser, Debug)]
#[command(name = "ci-job-runner", about = "Submit jobs to CI Controller")]
struct Cli {
    /// Path to controller YAML config file
    #[arg(short, long)]
    config: String,

    /// Job ID
    #[arg(short = 'i', long, default_value = "job-001")]
    job_id: String,

    /// Job type
    #[arg(short = 't', long, default_value = "common")]
    job_type: String,

    /// Command to execute
    #[arg(required = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("✗ Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Load controller config to get the bind address
    let config = ControllerConfig::from_file(&cli.config)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    // Construct controller URL from bind_address
    // bind_address is like "0.0.0.0:50051", we need "http://localhost:50051"
    let controller_addr = config.bind_address.replace("0.0.0.0", "localhost");
    let controller_url = format!("http://{}", controller_addr);

    let command = cli.command.join(" ");
    info!("Connecting to controller at {}", controller_url);
    info!("Submitting job: {}", cli.job_id);
    info!("Command: {}", command);

    // Connect to controller
    let channel = Channel::from_shared(controller_url)?.connect().await?;

    let mut client = OrchestratorClient::new(channel);

    // Submit the job
    let request = tonic::Request::new(SubmitJobRequest {
        job_id: cli.job_id.clone(),
        command: command.clone(),
        job_type: cli.job_type.clone(),
        required_cpu: 1,
        required_memory_mb: 1024,
        required_disk_mb: 1024,
        isolation_required: false,
        branch_id: "".to_string(),
        environment: std::collections::HashMap::new(),
    });

    let response = client.submit_job(request).await?;
    let resp = response.into_inner();

    if resp.accepted {
        info!("✓ Job accepted: {}", resp.job_id);
        info!("  Command: {}", command);
        info!("  Message: {}", resp.message);
        info!("Streaming logs... (Ctrl+C to cancel)");
        println!("{}", "-".repeat(60));

        // Open WatchJobLogs stream to receive logs in real-time
        let log_request = tonic::Request::new(WatchJobLogsRequest {
            job_id: cli.job_id.clone(),
            from_offset: 0,
        });

        match client.watch_job_logs(log_request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                let job_id = cli.job_id.clone();

                // Use tokio::select! to handle both log streaming and Ctrl+C
                tokio::select! {
                    // Log streaming path
                    result = async {
                        // Use tonic's Streaming interface directly
                        while let Some(chunk_result) = stream.message().await? {
                            // Print log data as it arrives
                            if !chunk_result.data.is_empty() {
                                let log_str = String::from_utf8_lossy(&chunk_result.data);
                                print!("{}", log_str);
                                // Flush stdout for immediate output
                                use std::io::Write;
                                std::io::stdout().flush().ok();
                            }
                        }
                        Ok::<(), anyhow::Error>(())
                    } => {
                        match result {
                            Ok(()) => {
                                println!("{}", "-".repeat(60));
                                info!("✓ Job {} completed", job_id);
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }

                    // Ctrl+C handler
                    _ = tokio::signal::ctrl_c() => {
                        println!("\n{}", "-".repeat(60));
                        info!("Received Ctrl+C, cancelling job {}...", job_id);

                        // Send CancelJob request
                        let cancel_request = tonic::Request::new(CancelJobRequest {
                            job_id: job_id.clone(),
                            reason: "User interrupted (Ctrl+C)".to_string(),
                        });

                        match client.cancel_job(cancel_request).await {
                            Ok(cancel_response) => {
                                let cancel_resp = cancel_response.into_inner();
                                if cancel_resp.accepted {
                                    info!("✓ Job {} cancellation requested: {}", job_id, cancel_resp.message);
                                    info!("Waiting for job to terminate...");

                                    // Wait for the job to reach terminal state
                                    let final_state = wait_for_job_termination(&mut client, &job_id).await;
                                    match final_state {
                                        Ok(state) => {
                                            info!("✓ Job {} terminated with state: {}", job_id, state);
                                        }
                                        Err(e) => {
                                            warn!("✗ Error waiting for job termination: {}", e);
                                        }
                                    }
                                } else {
                                    warn!("⚠ Cancel request not accepted: {}", cancel_resp.message);
                                }
                            }
                            Err(e) => {
                                warn!("✗ Failed to cancel job: {}", e);
                            }
                        }

                        Err(anyhow::anyhow!("Job cancelled by user"))
                    }
                }
            }
            Err(e) => {
                warn!("✗ Failed to watch job logs: {}", e);
                info!("Falling back to polling for status...");

                // Fallback to polling if WatchJobLogs is not available
                fallback_poll_status(&mut client, &cli.job_id).await
            }
        }
    } else {
        Err(anyhow::anyhow!("Job rejected: {}", resp.message))
    }
}

/// Wait for job to reach terminal state (SUCCESS, FAILED, or CANCELLED)
async fn wait_for_job_termination(
    client: &mut OrchestratorClient<Channel>,
    job_id: &str,
) -> anyhow::Result<String> {
    use ci_core::proto::orchestrator::{GetJobStatusRequest, JobState};

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        interval.tick().await;

        let status_request = tonic::Request::new(GetJobStatusRequest {
            job_id: job_id.to_string(),
        });

        match client.get_job_status(status_request).await {
            Ok(status_response) => {
                let status = status_response.into_inner();

                if !status.found {
                    return Err(anyhow::anyhow!("Job not found on controller"));
                }

                let state = JobState::try_from(status.state).unwrap_or(JobState::Unknown);

                match state {
                    JobState::Success => return Ok("SUCCESS".to_string()),
                    JobState::Failed => return Ok("FAILED".to_string()),
                    JobState::Cancelled => return Ok("CANCELLED".to_string()),
                    _ => {
                        // Still in progress (Queued, Assigned, Running)
                        continue;
                    }
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query job status: {}", e));
            }
        }
    }
}

/// Fallback to polling for job status if WatchJobLogs fails
async fn fallback_poll_status(
    client: &mut OrchestratorClient<Channel>,
    job_id: &str,
) -> anyhow::Result<()> {
    use ci_core::proto::orchestrator::{GetJobStatusRequest, JobState};

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let status_request = tonic::Request::new(GetJobStatusRequest {
            job_id: job_id.to_string(),
        });

        match client.get_job_status(status_request).await {
            Ok(status_response) => {
                let status = status_response.into_inner();

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
                            "✓ Job {} completed successfully (exit code: {})",
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
                    _ => {
                        // Still in progress (Queued, Assigned, Running)
                        continue;
                    }
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to query job status: {}", e));
            }
        }
    }
}
