use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::info;

/// Worker metrics for Prometheus exposition
#[derive(Clone)]
pub struct WorkerMetrics {
    pub jobs_executed: Arc<AtomicU64>,
    pub jobs_succeeded: Arc<AtomicU64>,
    pub jobs_failed: Arc<AtomicU64>,
    pub jobs_cancelled: Arc<AtomicU64>,
    pub active_jobs: Arc<AtomicI64>,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        Self {
            jobs_executed: Arc::new(AtomicU64::new(0)),
            jobs_succeeded: Arc::new(AtomicU64::new(0)),
            jobs_failed: Arc::new(AtomicU64::new(0)),
            jobs_cancelled: Arc::new(AtomicU64::new(0)),
            active_jobs: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn to_prometheus(&self) -> String {
        format!(
            "# HELP ci_worker_up Worker is running\n\
             # TYPE ci_worker_up gauge\n\
             ci_worker_up 1\n\
             # HELP ci_worker_jobs_executed_total Total jobs executed\n\
             # TYPE ci_worker_jobs_executed_total counter\n\
             ci_worker_jobs_executed_total {}\n\
             # HELP ci_worker_jobs_succeeded_total Jobs completed successfully\n\
             # TYPE ci_worker_jobs_succeeded_total counter\n\
             ci_worker_jobs_succeeded_total {}\n\
             # HELP ci_worker_jobs_failed_total Jobs that failed\n\
             # TYPE ci_worker_jobs_failed_total counter\n\
             ci_worker_jobs_failed_total {}\n\
             # HELP ci_worker_jobs_cancelled_total Jobs that were cancelled\n\
             # TYPE ci_worker_jobs_cancelled_total counter\n\
             ci_worker_jobs_cancelled_total {}\n\
             # HELP ci_worker_active_jobs Currently running jobs\n\
             # TYPE ci_worker_active_jobs gauge\n\
             ci_worker_active_jobs {}\n",
            self.jobs_executed.load(Ordering::Relaxed),
            self.jobs_succeeded.load(Ordering::Relaxed),
            self.jobs_failed.load(Ordering::Relaxed),
            self.jobs_cancelled.load(Ordering::Relaxed),
            self.active_jobs.load(Ordering::Relaxed),
        )
    }
}

async fn health_live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn health_ready() -> Json<Value> {
    Json(json!({"status": "ok", "message": "worker ready"}))
}

async fn metrics(State(worker_metrics): State<WorkerMetrics>) -> String {
    worker_metrics.to_prometheus()
}

pub async fn run(http_addr: SocketAddr, worker_metrics: WorkerMetrics) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/metrics", get(metrics))
        .with_state(worker_metrics);

    info!("Worker HTTP server listening on {}", http_addr);
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
