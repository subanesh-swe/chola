use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tracing::info;

use ci_core::proto::orchestrator::{
    orchestrator_client::OrchestratorClient, HeartbeatAck, HeartbeatMessage, JobAssignment,
    JobStatusAck, JobStatusUpdate, JobStreamRequest, LogAck, LogChunk, ReconnectRequest,
    ReconnectResponse, RegisterRequest, RegisterResponse,
};

/// gRPC client for connecting to controller
#[derive(Clone)]
pub struct GrpcClient {
    client: Arc<Mutex<OrchestratorClient<Channel>>>,
}

impl GrpcClient {
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        info!("Connecting to controller at {}", addr);
        let client = OrchestratorClient::connect(addr.to_string()).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub async fn register(&self, req: RegisterRequest) -> anyhow::Result<RegisterResponse> {
        let mut client = self.client.lock().await;
        let resp = client.register(tonic::Request::new(req)).await?;
        Ok(resp.into_inner())
    }

    pub async fn heartbeat(
        &self,
        rx: tokio::sync::mpsc::Receiver<HeartbeatMessage>,
    ) -> anyhow::Result<tonic::Streaming<HeartbeatAck>> {
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let mut client = self.client.lock().await;
        let resp = client.heartbeat(tonic::Request::new(stream)).await?;
        Ok(resp.into_inner())
    }

    pub async fn job_stream(
        &self,
        worker_id: String,
    ) -> anyhow::Result<tonic::Streaming<JobAssignment>> {
        let req = JobStreamRequest { worker_id };
        let mut client = self.client.lock().await;
        let resp = client.job_stream(tonic::Request::new(req)).await?;
        Ok(resp.into_inner())
    }

    pub async fn report_job_status(&self, update: JobStatusUpdate) -> anyhow::Result<JobStatusAck> {
        let mut client = self.client.lock().await;
        let resp = client
            .report_job_status(tonic::Request::new(update))
            .await?;
        Ok(resp.into_inner())
    }

    /// Open a client-streaming StreamLogs RPC call.
    ///
    /// Consumes LogChunks from the provided channel and streams them to the controller.
    /// Returns the LogAck from the controller when the stream completes.
    pub async fn stream_logs(
        &self,
        rx: tokio::sync::mpsc::Receiver<LogChunk>,
    ) -> anyhow::Result<LogAck> {
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let mut client = self.client.lock().await;
        let resp = client.stream_logs(tonic::Request::new(stream)).await?;
        Ok(resp.into_inner())
    }

    /// Reconnect to controller after a disconnect.
    ///
    /// Sends the current worker state including running jobs to the controller
    /// for reconciliation. Returns directives for log resumption if needed.
    pub async fn reconnect(&self, req: ReconnectRequest) -> anyhow::Result<ReconnectResponse> {
        let mut client = self.client.lock().await;
        let resp = client.reconnect(tonic::Request::new(req)).await?;
        Ok(resp.into_inner())
    }
}
