use ci_core::proto::orchestrator::{
    orchestrator_client::OrchestratorClient, CancelJobRequest, JobState, SubmitStageRequest,
    WatchJobLogsRequest,
};
use tonic::transport::Channel;
use tracing::{info, warn};

use super::submit::{
    exit_with_job_status, fallback_poll_status, stream_logs, wait_for_job_termination,
};

pub async fn execute(
    client: &mut OrchestratorClient<Channel>,
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
                    warn!("Received Ctrl+C, cancelling job {}...", jid);

                    let cancel = tonic::Request::new(CancelJobRequest {
                        job_id: jid.clone(),
                        reason: "User interrupted (Ctrl+C)".to_string(),
                        job_group_id: job_group_id.clone(),
                    });

                    match client.cancel_job(cancel).await {
                        Ok(cr) => {
                            let cr = cr.into_inner();
                            if cr.accepted {
                                info!("Cancellation accepted: {}", cr.message);
                                match wait_for_job_termination(client, &jid).await {
                                    Ok(s) => info!(
                                        "Job {} terminated with state: {:?}",
                                        jid,
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

                    Err(anyhow::anyhow!("Stage cancelled by user"))
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
