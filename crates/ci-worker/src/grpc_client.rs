use std::time::Duration;
use tonic::metadata::MetadataValue;
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
    auth_token: Option<String>,
}

impl GrpcClient {
    #[allow(dead_code)]
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        Self::connect_with_options(addr, None, None).await
    }

    #[allow(dead_code)]
    pub async fn connect_with_tls(
        addr: &str,
        tls_config: Option<&ci_core::models::config::TlsClientConfig>,
    ) -> anyhow::Result<Self> {
        Self::connect_with_options(addr, tls_config, None).await
    }

    pub async fn connect_with_options(
        addr: &str,
        tls_config: Option<&ci_core::models::config::TlsClientConfig>,
        auth_token: Option<String>,
    ) -> anyhow::Result<Self> {
        info!("Connecting to controller at {}", addr);

        let mut endpoint = tonic::transport::Channel::from_shared(addr.to_string())?
            .http2_keep_alive_interval(Duration::from_secs(10))
            .keep_alive_timeout(Duration::from_secs(20))
            .keep_alive_while_idle(true);

        if let Some(tls) = tls_config {
            if tls.enabled {
                let mut tls_conf = tonic::transport::ClientTlsConfig::new();

                // Load CA certificate
                if let Some(ref ca_path) = tls.ca_cert {
                    let ca = tokio::fs::read(ca_path).await?;
                    let ca_cert = tonic::transport::Certificate::from_pem(ca);
                    tls_conf = tls_conf.ca_certificate(ca_cert);
                }

                // Load client certificate for mTLS
                if let Some(ref cert_path) = tls.client_cert {
                    if let Some(ref key_path) = tls.client_key {
                        let cert = tokio::fs::read(cert_path).await?;
                        let key = tokio::fs::read(key_path).await?;
                        let identity = tonic::transport::Identity::from_pem(cert, key);
                        tls_conf = tls_conf.identity(identity);
                    }
                }

                endpoint = endpoint.tls_config(tls_conf)?;
                info!("TLS enabled for controller connection");
            }
        }

        let channel = endpoint.connect().await?;
        let client = OrchestratorClient::new(channel);
        Ok(Self { client, auth_token })
    }

    /// Build a tonic Request with the auth token injected if configured.
    fn make_request<T>(&self, inner: T) -> tonic::Request<T> {
        let mut req = tonic::Request::new(inner);
        if let Some(ref token) = self.auth_token {
            if let Ok(val) = format!("Bearer {}", token).parse::<MetadataValue<_>>() {
                req.metadata_mut().insert("authorization", val);
            }
        }
        req
    }

    pub async fn register(&self, req: RegisterRequest) -> anyhow::Result<RegisterResponse> {
        let resp = self.client.clone().register(self.make_request(req)).await?;
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
            .heartbeat(self.make_request(stream))
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
            .job_stream(self.make_request(req))
            .await?;
        Ok(resp.into_inner())
    }

    pub async fn report_job_status(&self, update: JobStatusUpdate) -> anyhow::Result<JobStatusAck> {
        let resp = self
            .client
            .clone()
            .report_job_status(self.make_request(update))
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
            .stream_logs(self.make_request(stream))
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
            .reconnect(self.make_request(req))
            .await?;
        Ok(resp.into_inner())
    }
}
