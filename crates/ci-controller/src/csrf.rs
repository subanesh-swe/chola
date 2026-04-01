use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Reject cross-origin POST/PUT/DELETE unless Origin is localhost or absent.
/// Webhooks are exempt — they come from external services.
pub async fn csrf_middleware(req: Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();

    let is_mutating = matches!(method, Method::POST | Method::PUT | Method::DELETE);
    let is_webhook = path.starts_with("/api/v1/webhooks");

    if is_mutating && !is_webhook {
        let origin = req
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let allowed = origin.is_empty()
            || origin.starts_with("http://localhost:")
            || origin.starts_with("https://localhost:")
            || origin == "http://localhost"
            || origin == "https://localhost";

        if !allowed {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    next.run(req).await
}
