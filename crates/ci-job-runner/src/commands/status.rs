use ci_core::proto::orchestrator::{
    orchestrator_client::OrchestratorClient, GetJobGroupStatusRequest,
};
use tonic::transport::Channel;
use tracing::info;

pub async fn execute(
    client: &mut OrchestratorClient<Channel>,
    job_group_id: String,
) -> anyhow::Result<()> {
    info!("Querying status for job_group_id={}", job_group_id);

    let request = tonic::Request::new(GetJobGroupStatusRequest {
        job_group_id: job_group_id.clone(),
    });

    let response = client.get_job_group_status(request).await?;
    let resp = response.into_inner();

    println!("Job Group: {}", resp.job_group_id);
    println!("State:     {}", resp.state);
    println!("Worker:    {}", resp.worker_id);
    println!("{}", "-".repeat(60));
    println!(
        "{:<20} {:<36} {:<12} {:<6}",
        "STAGE", "JOB ID", "STATE", "EXIT"
    );
    println!("{}", "-".repeat(60));

    for stage in &resp.stages {
        println!(
            "{:<20} {:<36} {:<12} {:<6}",
            stage.stage_name, stage.job_id, stage.state, stage.exit_code
        );
    }

    println!("{}", "-".repeat(60));
    Ok(())
}
