pub mod cancel;
pub mod logs;
pub mod reserve;
pub mod run;
pub mod status;
pub mod submit;

use ci_core::proto::orchestrator::orchestrator_client::OrchestratorClient;
use tonic::transport::Channel;

pub async fn connect(controller: &str) -> anyhow::Result<OrchestratorClient<Channel>> {
    let channel = Channel::from_shared(controller.to_string())?
        .connect()
        .await?;
    Ok(OrchestratorClient::new(channel))
}
