use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::info;

use crate::job_group_registry::JobGroupRegistry;
use crate::monitoring::Metrics;
use crate::worker_registry::WorkerRegistry;

// ---------------------------------------------------------------------------
// Shared HTTP state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub worker_registry: Arc<RwLock<WorkerRegistry>>,
    pub job_group_registry: Arc<RwLock<JobGroupRegistry>>,
    pub metrics: Metrics,
}

// ---------------------------------------------------------------------------
// Health endpoints
// ---------------------------------------------------------------------------

/// GET /health/live — always 200 while the process is alive.
async fn health_live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

/// GET /health/ready — 200 when the server has at least initialised its
/// registries.  Full dependency checks (Redis, Postgres) can be layered in
/// later; for now this is equivalent to liveness but semantically distinct.
async fn health_ready() -> Json<Value> {
    Json(json!({"status": "ok", "message": "controller ready"}))
}

// ---------------------------------------------------------------------------
// Metrics endpoint
// ---------------------------------------------------------------------------

/// GET /metrics — Prometheus text format.
async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.metrics.to_prometheus();
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

// ---------------------------------------------------------------------------
// API: workers
// ---------------------------------------------------------------------------

/// GET /api/v1/workers — JSON list of all known workers with their status.
async fn api_workers(State(state): State<AppState>) -> Json<Value> {
    let registry = state.worker_registry.read().await;
    let workers: Vec<Value> = registry
        .all_workers()
        .into_iter()
        .map(|w| {
            let last_hb = w.last_heartbeat.as_ref().map(|hb| {
                json!({
                    "used_cpu_percent": hb.used_cpu_percent,
                    "used_memory_mb": hb.used_memory_mb,
                    "used_disk_mb": hb.used_disk_mb,
                    "running_jobs": hb.running_job_ids.len(),
                    "system_load": hb.system_load,
                    "timestamp": hb.timestamp.to_rfc3339(),
                })
            });

            json!({
                "worker_id": w.info.worker_id,
                "hostname": w.info.hostname,
                "status": format!("{:?}", w.status),
                "total_cpu": w.info.total_cpu,
                "total_memory_mb": w.info.total_memory_mb,
                "total_disk_mb": w.info.total_disk_mb,
                "disk_type": w.info.disk_type.to_string(),
                "docker_enabled": w.info.docker_enabled,
                "supported_job_types": w.info.supported_job_types,
                "registered_at": w.registered_at.to_rfc3339(),
                "last_heartbeat": last_hb,
            })
        })
        .collect();

    Json(json!({ "workers": workers, "count": workers.len() }))
}

/// POST /api/v1/workers/:id/drain — put a worker into drain mode.
async fn api_drain_worker(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
) -> impl IntoResponse {
    let mut registry = state.worker_registry.write().await;
    if registry.mark_draining(&worker_id) {
        info!("Worker {} set to drain mode via API", worker_id);
        (
            StatusCode::OK,
            Json(json!({
                "worker_id": worker_id,
                "status": "draining",
                "message": "Worker will finish current jobs then disconnect",
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("Worker '{}' not found", worker_id),
            })),
        )
    }
}

// ---------------------------------------------------------------------------
// API: builds
// ---------------------------------------------------------------------------

/// GET /api/v1/builds — JSON list of active (non-terminal) job groups.
async fn api_builds(State(state): State<AppState>) -> Json<Value> {
    let registry = state.job_group_registry.read().await;
    // Collect every group that is not yet in a terminal state
    let builds: Vec<Value> = registry
        .active_groups()
        .into_iter()
        .map(|g| {
            json!({
                "job_group_id": g.id.to_string(),
                "state": g.state.to_string(),
                "worker_id": g.reserved_worker_id,
                "branch": g.branch,
                "commit_sha": g.commit_sha,
                "created_at": g.created_at.to_rfc3339(),
                "updated_at": g.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Json(json!({ "builds": builds, "count": builds.len() }))
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Start the HTTP sidecar server.
///
/// Accepts shared registry handles and a `Metrics` instance so all endpoints
/// serve real data rather than static placeholders.
pub async fn run(
    http_addr: SocketAddr,
    worker_registry: Arc<RwLock<WorkerRegistry>>,
    job_group_registry: Arc<RwLock<JobGroupRegistry>>,
    metrics: Metrics,
) -> anyhow::Result<()> {
    let state = AppState {
        worker_registry,
        job_group_registry,
        metrics,
    };

    let app = Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/metrics", get(metrics_handler))
        .route("/api/v1/workers", get(api_workers))
        .route("/api/v1/workers/:id/drain", post(api_drain_worker))
        .route("/api/v1/builds", get(api_builds))
        .with_state(state);

    info!("HTTP server listening on {}", http_addr);
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// Rename the handler to avoid conflict with the `metrics` field name in AppState
async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    metrics(State(state)).await
}
