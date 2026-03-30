use std::io::Write;

use ci_core::proto::orchestrator::WatchJobLogsRequest;
use tracing::info;

pub async fn execute(
    client: &mut super::Client,
    job_group_id: Option<String>,
    job_id: Option<String>,
    stage: Option<String>,
) -> anyhow::Result<()> {
    if job_group_id.is_none() && job_id.is_none() {
        return Err(anyhow::anyhow!(
            "At least one of --job-group-id or --job-id must be provided"
        ));
    }

    let request = tonic::Request::new(WatchJobLogsRequest {
        job_id: job_id.clone().unwrap_or_default(),
        from_offset: 0,
        job_group_id: job_group_id.clone().unwrap_or_default(),
        stage_name: stage.unwrap_or_default(),
    });

    info!(
        "Watching logs (job_group_id={}, job_id={})...",
        job_group_id.as_deref().unwrap_or("-"),
        job_id.as_deref().unwrap_or("-")
    );
    println!("{}", "-".repeat(60));

    let response = client.watch_job_logs(request).await?;
    let mut stream = response.into_inner();

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
            println!("{}", "-".repeat(60));
            result
        }

        _ = tokio::signal::ctrl_c() => {
            println!("\n{}", "-".repeat(60));
            info!("Log streaming stopped by user.");
            Ok(())
        }
    }
}
