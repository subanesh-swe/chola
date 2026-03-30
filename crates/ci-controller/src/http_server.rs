use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue};
use axum::{
    extract::{Path, State},
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::state::ControllerState;

// ---------------------------------------------------------------------------
// Health endpoints
// ---------------------------------------------------------------------------

/// GET /health/live -- always 200 while the process is alive.
async fn health_live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

/// GET /health/ready -- 200 when the server has at least initialised its
/// registries.
async fn health_ready() -> Json<Value> {
    Json(json!({"status": "ok", "message": "controller ready"}))
}

// ---------------------------------------------------------------------------
// Metrics endpoint
// ---------------------------------------------------------------------------

/// GET /metrics -- Prometheus text format.
async fn metrics_handler(State(state): State<Arc<ControllerState>>) -> impl IntoResponse {
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
// Legacy API: workers  (kept for backwards-compat, also available via /api/v1)
// ---------------------------------------------------------------------------

/// GET /api/v1/workers -- JSON list of all known workers with their status.
async fn api_workers(State(state): State<Arc<ControllerState>>) -> Json<Value> {
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

/// POST /api/v1/workers/:id/drain -- put a worker into drain mode.
async fn api_drain_worker(
    State(state): State<Arc<ControllerState>>,
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
// Legacy API: builds
// ---------------------------------------------------------------------------

/// GET /api/v1/builds -- JSON list of active (non-terminal) job groups.
async fn api_builds(State(state): State<Arc<ControllerState>>) -> Json<Value> {
    let registry = state.job_group_registry.read().await;
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
pub async fn run(http_addr: SocketAddr, state: Arc<ControllerState>) -> anyhow::Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            let s = origin.to_str().unwrap_or("");
            s.starts_with("http://localhost:") || s.starts_with("https://localhost:")
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
        ]);

    let api = crate::api::api_router();

    let app = Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/metrics", get(metrics_handler))
        // REST API (includes auth, users, repos, builds, workers, jobs, logs, dashboard)
        .nest("/api/v1", api)
        .layer(cors)
        .with_state(state);

    info!("HTTP server listening on {}", http_addr);
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
