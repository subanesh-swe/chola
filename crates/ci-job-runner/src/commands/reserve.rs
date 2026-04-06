use ci_core::proto::orchestrator::ReserveWorkerRequest;
use tracing::info;

pub async fn execute(
    client: &mut super::Client,
    repo: String,
    repo_url: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
    stages: Vec<String>,
    idempotency_key: Option<String>,
) -> anyhow::Result<()> {
    info!("Reserving worker for repo={} stages={:?}", repo, stages);

    let request = tonic::Request::new(ReserveWorkerRequest {
        repo_name: repo,
        repo_url: repo_url.unwrap_or_default(),
        branch: branch.unwrap_or_default(),
        commit_sha: commit.unwrap_or_default(),
        stages,
        priority: 0,
        idempotency_key: idempotency_key.unwrap_or_default(),
    });

    let response = client.reserve_worker(request).await?;
    let resp = response.into_inner();

    if resp.success {
        let stage_names: Vec<String> = resp.stages.iter().map(|s| s.stage_name.clone()).collect();
        println!(
            "job_group_id={} worker={} stages={}",
            resp.job_group_id,
            resp.worker_id,
            stage_names.join(",")
        );
        info!("Reservation successful: {}", resp.message);
    } else {
        return Err(anyhow::anyhow!("Reservation failed: {}", resp.message));
    }

    Ok(())
}
