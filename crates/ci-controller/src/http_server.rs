use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue, Uri};
use axum::middleware;
use axum::{
    extract::State,
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
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

fn build_app(state: Arc<ControllerState>, frontend_dir: Option<PathBuf>) -> Router {
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

    let base = base.layer(middleware::from_fn(csrf_middleware)).layer(cors);

    // When a built frontend is configured, serve it as a SPA from the
    // fallback; otherwise keep the JSON 404 (API-only). Canonicalize the dir
    // once so the per-request traversal check is a cheap prefix compare.
    let app = match frontend_dir.and_then(|d| std::fs::canonicalize(d).ok()) {
        Some(dir) if dir.is_dir() => {
            info!("Serving frontend from {}", dir.display());
            let index = dir.join("index.html");
            base.fallback(move |uri: Uri| {
                let dir = dir.clone();
                let index = index.clone();
                async move { serve_spa(uri, dir, index).await }
            })
        }
        _ => base.fallback(fallback_handler),
    };
    app.with_state(state)
}

/// SPA fallback: serve a static file from `dir` if it exists (and is safely
/// under `dir`), otherwise `index.html` so client-side routes resolve. API /
/// infra paths never fall back to HTML — they get a JSON 404.
async fn serve_spa(uri: Uri, dir: PathBuf, index: PathBuf) -> Response {
    let path = uri.path();
    if path.starts_with("/api")
        || path.starts_with("/health")
        || path == "/metrics"
        || path.starts_with("/swagger-ui")
        || path.starts_with("/api-docs")
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Route not found: {}", path) })),
        )
            .into_response();
    }

    // Resolve the requested file; fall back to index.html for SPA routes and
    // reject path traversal (canonical path must stay under `dir`).
    let candidate = dir.join(path.trim_start_matches('/'));
    let serve_path = match tokio::fs::canonicalize(&candidate).await {
        Ok(c) if c.starts_with(&dir) && c.is_file() => c,
        _ => index,
    };

    match tokio::fs::read(&serve_path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&serve_path).first_or_octet_stream();
            ([(axum::http::header::CONTENT_TYPE, mime.as_ref())], bytes).into_response()
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "frontend index.html not found" })),
        )
            .into_response(),
    }
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
    frontend_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let app = build_app(state, frontend_dir);
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
