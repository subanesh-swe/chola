use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Notify, RwLock};
use tonic::Status;

use ci_core::models::config::ControllerConfig;
use ci_core::proto::orchestrator::JobAssignment;

use crate::auth::middleware::AuthConfig;
use crate::job_group_registry::JobGroupRegistry;
use crate::job_registry::JobRegistry;
use crate::log_aggregator::LogAggregator;
use crate::monitoring::Metrics;
use crate::worker_registry::WorkerRegistry;

/// Shared controller state used by both the gRPC and HTTP servers.
pub struct ControllerState {
    pub config: ControllerConfig,
    /// Auth config extracted from ControllerConfig for the middleware extractor.
    pub auth_config: AuthConfig,
    /// Worker registry -- shared with the HTTP sidecar via `Arc`.
    pub worker_registry: Arc<RwLock<WorkerRegistry>>,
    pub job_registry: RwLock<JobRegistry>,
    pub log_aggregator: RwLock<LogAggregator>,
    /// Job-group registry -- shared with the HTTP sidecar via `Arc`.
    pub job_group_registry: Arc<RwLock<JobGroupRegistry>>,
    /// Channel to send job assignments (including cancel directives) to workers (worker_id -> sender)
    pub job_stream_senders:
        RwLock<HashMap<String, tokio::sync::mpsc::Sender<Result<JobAssignment, Status>>>>,
    /// Notify to wake the scheduler when a job is submitted or worker state changes
    pub scheduler_notify: Notify,
    /// Prometheus-compatible metrics -- shared with the HTTP sidecar via `Clone`.
    pub metrics: Metrics,
    /// PostgreSQL storage (None if unavailable)
    pub storage: Option<Arc<crate::storage::Storage>>,
    /// Redis store (None if unavailable)
    pub redis_store: Option<Arc<crate::redis_store::RedisStore>>,
    /// Directory where logs are persisted on disk (always set)
    pub log_dir: String,
}

/// Allow the axum auth middleware extractor to pull `AuthConfig` from `Arc<ControllerState>`.
impl AsRef<AuthConfig> for Arc<ControllerState> {
    fn as_ref(&self) -> &AuthConfig {
        &self.auth_config
    }
}
