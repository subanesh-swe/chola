use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue};
use axum::middleware;
use axum::{
    extract::State,
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::csrf::csrf_middleware;
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
        .layer(middleware::from_fn(csrf_middleware))
        .layer(cors)
        .with_state(state);

    info!("HTTP server listening on {}", http_addr);
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
