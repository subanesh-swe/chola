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
use ci_core::models::config::TlsServerConfig;
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::info;

use utoipa::OpenApi;
#[cfg(feature = "swagger-ui")]
use utoipa_swagger_ui::SwaggerUi;

use crate::csrf::csrf_middleware;
use crate::state::ControllerState;

// ---------------------------------------------------------------------------
// Health endpoints
// ---------------------------------------------------------------------------

async fn health_live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn health_ready() -> Json<Value> {
    Json(json!({"status": "ok", "message": "controller ready"}))
}

// ---------------------------------------------------------------------------
// Metrics endpoint
// ---------------------------------------------------------------------------

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
// Router builder
// ---------------------------------------------------------------------------

fn build_app(state: Arc<ControllerState>) -> Router {
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
    let public_api = crate::api::public_api_router();

    let base = Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/metrics", get(metrics_handler))
        // Public routes (badge SVG) — no CSRF/auth
        .nest("/api/v1", public_api.with_state(state.clone()))
        // Authenticated API — wrap errors in JSON
        .nest(
            "/api/v1",
            api.layer(middleware::from_fn(json_error_wrapper)),
        );

    // OpenAPI / Swagger UI (disabled in production builds without swagger-ui feature)
    #[cfg(feature = "swagger-ui")]
    let base = base.merge(
        SwaggerUi::new("/swagger-ui")
            .url("/api-docs/openapi.json", crate::openapi::ApiDoc::openapi()),
    );

    base.layer(middleware::from_fn(csrf_middleware))
        .layer(cors)
        .fallback(fallback_handler)
        .with_state(state)
}

/// Middleware: if the response is a 4xx/5xx with non-JSON content-type, wrap in JSON.
async fn json_error_wrapper(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let resp = next.run(req).await;
    let status = resp.status();
    if status.is_client_error() || status.is_server_error() {
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type.contains("application/json") && !content_type.contains("image/svg") {
            let body_bytes = axum::body::to_bytes(resp.into_body(), 4096)
                .await
                .unwrap_or_default();
            let text = String::from_utf8_lossy(&body_bytes);
            return (status, Json(json!({"error": text.trim().to_string()}))).into_response();
        }
    }
    resp
}

/// Catch-all handler: returns JSON for any unmatched route or path parse error.
async fn fallback_handler(uri: axum::http::Uri) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": format!("Route not found: {}", uri.path())})),
    )
}

// ---------------------------------------------------------------------------
// Server startup
// ---------------------------------------------------------------------------

/// Start the HTTP sidecar. Uses TLS when `tls_config` is Some and enabled.
pub async fn run(
    http_addr: SocketAddr,
    state: Arc<ControllerState>,
    tls_config: Option<TlsServerConfig>,
) -> anyhow::Result<()> {
    let app = build_app(state);
    match tls_config {
        Some(tls) if tls.enabled => serve_tls(http_addr, app, &tls).await,
        _ => serve_plain(http_addr, app).await,
    }
}

async fn serve_plain(addr: SocketAddr, app: Router) -> anyhow::Result<()> {
    info!("HTTP server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn serve_tls(addr: SocketAddr, app: Router, tls: &TlsServerConfig) -> anyhow::Result<()> {
    let cert = tls
        .server_cert
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("http_tls.server_cert required when TLS is enabled"))?;
    let key = tls
        .server_key
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("http_tls.server_key required when TLS is enabled"))?;

    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;

    let hsts = HeaderValue::from_static("max-age=31536000; includeSubDomains");
    let app = app.layer(SetResponseHeaderLayer::if_not_present(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        hsts,
    ));

    info!("HTTP server (TLS) listening on https://{}", addr);
    axum_server::bind_rustls(addr, rustls_config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
}
