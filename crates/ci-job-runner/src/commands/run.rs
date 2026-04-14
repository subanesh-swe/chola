use ci_core::proto::orchestrator::{
    CancelJobRequest, JobState, SubmitStageRequest, WatchJobLogsRequest,
};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};

use super::submit::{
    exit_with_job_status, fallback_poll_status, stream_logs, wait_for_job_termination_with_timeout,
};

pub async fn execute(
    client: &mut super::Client,
    job_group_id: String,
    job_id: Option<String>,
    stage: String,
    command_override: Option<String>,
) -> anyhow::Result<()> {
    let job_id = job_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    info!(
        "Submitting stage '{}' in group '{}' (job_id={})",
        stage, job_group_id, job_id
    );

    let request = tonic::Request::new(SubmitStageRequest {
        job_group_id: job_group_id.clone(),
        job_id: job_id.clone(),
        stage_name: stage.clone(),
        command_override: command_override.unwrap_or_default(),
        environment: std::collections::HashMap::new(),
    });

    let response = client.submit_stage(request).await?;
    let resp = response.into_inner();

    if !resp.accepted {
        return Err(anyhow::anyhow!(
            "Stage '{}' rejected: {}",
            resp.stage_name,
            resp.message
        ));
    }

    info!("Stage accepted (job_id={}). Streaming logs...", resp.job_id);
    println!("{}", "-".repeat(60));

    let log_request = tonic::Request::new(WatchJobLogsRequest {
        job_id: job_id.clone(),
        from_offset: 0,
        job_group_id: job_group_id.clone(),
        stage_name: stage.clone(),
    });

    let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");

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
                    cancel_and_exit(client, &jid, &job_group_id, "User interrupted (Ctrl+C)", 130).await;
                }

                _ = sigterm.recv() => {
                    cancel_and_exit(client, &jid, &job_group_id, "Process terminated (SIGTERM)", 143).await;
                }
            }
        }
        Err(e) => {
            warn!("Failed to watch logs: {}", e);
            info!("Falling back to status polling...");
            fallback_poll_status(client, &job_id).await
        }
    }
}

async fn cancel_and_exit(
    client: &mut super::Client,
    job_id: &str,
    job_group_id: &str,
    reason: &str,
    exit_code: i32,
) -> ! {
    println!("\n{}", "-".repeat(60));
    warn!("{}, cancelling job {}...", reason, job_id);

    let cancel = tonic::Request::new(CancelJobRequest {
        job_id: job_id.to_string(),
        reason: reason.to_string(),
        job_group_id: job_group_id.to_string(),
    });

    match client.cancel_job(cancel).await {
        Ok(cr) => {
            let cr = cr.into_inner();
            if cr.accepted {
                info!("Cancellation accepted: {}", cr.message);
                match wait_for_job_termination_with_timeout(client, job_id, 30).await {
                    Ok(s) => info!(
                        "Job {} terminated with state: {:?}",
                        job_id,
                        JobState::try_from(s.state).unwrap_or(JobState::Unknown)
                    ),
                    Err(_) => warn!("Job {} not terminated after 30s, exiting anyway", job_id),
                }
            } else {
                warn!("Cancel not accepted: {}", cr.message);
            }
        }
        Err(e) => warn!("Failed to cancel job: {}", e),
    }

    std::process::exit(exit_code)
}
