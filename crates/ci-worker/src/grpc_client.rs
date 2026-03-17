use tonic::transport::Channel;
use tracing::info;

use ci_core::proto::orchestrator::{
    orchestrator_client::OrchestratorClient, HeartbeatAck, HeartbeatMessage, JobAssignment,
    JobStatusAck, JobStatusUpdate, JobStreamRequest, LogAck, LogChunk, ReconnectRequest,
    ReconnectResponse, RegisterRequest, RegisterResponse,
};

/// gRPC client for connecting to controller.
///
/// The tonic `OrchestratorClient<Channel>` is internally thread-safe:
/// the underlying `Channel` multiplexes concurrent RPCs over a single
/// HTTP/2 connection. No `Mutex` is needed, and using one would block
/// concurrent streaming calls (e.g., multiple jobs streaming logs).
#[derive(Clone)]
pub struct GrpcClient {
    client: OrchestratorClient<Channel>,
}

impl GrpcClient {
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        info!("Connecting to controller at {}", addr);
        let client = OrchestratorClient::connect(addr.to_string()).await?;
        Ok(Self { client })
    }

    pub async fn register(&self, req: RegisterRequest) -> anyhow::Result<RegisterResponse> {
        let resp = self
            .client
            .clone()
            .register(tonic::Request::new(req))
            .await?;
        Ok(resp.into_inner())
    }

    pub async fn heartbeat(
        &self,
        rx: tokio::sync::mpsc::Receiver<HeartbeatMessage>,
    ) -> anyhow::Result<tonic::Streaming<HeartbeatAck>> {
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let resp = self
            .client
            .clone()
            .heartbeat(tonic::Request::new(stream))
            .await?;
        Ok(resp.into_inner())
    }

    pub async fn job_stream(
        &self,
        worker_id: String,
    ) -> anyhow::Result<tonic::Streaming<JobAssignment>> {
        let req = JobStreamRequest { worker_id };
        let resp = self
            .client
            .clone()
            .job_stream(tonic::Request::new(req))
            .await?;
        Ok(resp.into_inner())
    }

    pub async fn report_job_status(&self, update: JobStatusUpdate) -> anyhow::Result<JobStatusAck> {
        let resp = self
            .client
            .clone()
            .report_job_status(tonic::Request::new(update))
            .await?;
        Ok(resp.into_inner())
    }

    /// Open a client-streaming StreamLogs RPC call.
    ///
    /// Consumes LogChunks from the provided channel and streams them to the controller.
    /// Returns the LogAck from the controller when the stream completes.
    ///
    /// NOTE: This is a long-running streaming call. The client clone allows concurrent
    /// jobs to stream logs simultaneously without blocking each other.
    pub async fn stream_logs(
        &self,
        rx: tokio::sync::mpsc::Receiver<LogChunk>,
    ) -> anyhow::Result<LogAck> {
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let resp = self
            .client
            .clone()
            .stream_logs(tonic::Request::new(stream))
            .await?;
        Ok(resp.into_inner())
    }

    /// Reconnect to controller after a disconnect.
    ///
    /// Sends the current worker state including running jobs to the controller
    /// for reconciliation. Returns directives for log resumption if needed.
    pub async fn reconnect(&self, req: ReconnectRequest) -> anyhow::Result<ReconnectResponse> {
        let resp = self
            .client
            .clone()
            .reconnect(tonic::Request::new(req))
            .await?;
        Ok(resp.into_inner())
    }
}
