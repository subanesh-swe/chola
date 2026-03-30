use ci_core::proto::orchestrator::CancelJobRequest;
use tracing::{error, info};

pub async fn execute(
    client: &mut super::Client,
    job_group_id: Option<String>,
    job_id: Option<String>,
    reason: String,
) -> anyhow::Result<()> {
    if job_group_id.is_none() && job_id.is_none() {
        return Err(anyhow::anyhow!(
            "At least one of --job-group-id or --job-id must be provided"
        ));
    }

    info!(
        "Cancelling (job_group_id={}, job_id={}) reason={}",
        job_group_id.as_deref().unwrap_or("-"),
        job_id.as_deref().unwrap_or("-"),
        reason
    );

    let request = tonic::Request::new(CancelJobRequest {
        job_id: job_id.unwrap_or_default(),
        reason,
        job_group_id: job_group_id.unwrap_or_default(),
    });

    let response = client.cancel_job(request).await?;
    let resp = response.into_inner();

    if resp.accepted {
        info!("Cancellation accepted: {}", resp.message);
    } else {
        error!("Cancellation not accepted: {}", resp.message);
        return Err(anyhow::anyhow!("Cancellation rejected: {}", resp.message));
    }

    Ok(())
}
