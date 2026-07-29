use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Reject cross-site mutating requests (CSRF). A request is allowed when:
///   - it's not mutating (GET/HEAD/OPTIONS), or it's a webhook (external callers), or
///   - it has no `Origin` header (non-browser clients / same-origin form posts), or
///   - it's **same-origin**: the `Origin` host matches the host the request
///     arrived on (`X-Forwarded-Host` from the proxy, else `Host`) — so any host
///     you serve the dashboard on (localhost, the machine IP, a Tailscale name)
///     works with no configuration, or
///   - the `Origin` is in the configured `allowed_origins` list (cross-origin opt-in).
pub async fn csrf_middleware(
    State(allowed): State<Arc<Vec<String>>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let is_mutating = matches!(*req.method(), Method::POST | Method::PUT | Method::DELETE);
    let is_webhook = req.uri().path().starts_with("/api/v1/webhooks");

    if is_mutating && !is_webhook {
        let headers = req.headers();
        let origin = headers
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !origin.is_empty() && !origin_allowed(origin, headers, &allowed) {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Cross-origin request blocked" })),
            )
                .into_response();
        }
    }

    next.run(req).await
}

/// True if `origin` is same-origin with the incoming request (its host equals
/// `X-Forwarded-Host` or `Host`), or is an exact match in the allowlist.
pub fn origin_allowed(origin: &str, headers: &HeaderMap, allowed: &[String]) -> bool {
    if allowed.iter().any(|a| a == origin) {
        return true;
    }
    let origin_host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
    let req_host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .map(|h| h.split(',').next().unwrap_or(h).trim())
        .unwrap_or("");
    !req_host.is_empty() && origin_host.eq_ignore_ascii_case(req_host)
}

#[cfg(test)]
mod tests {
    use super::origin_allowed;
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            let name: HeaderName = k.parse().unwrap();
            let val: HeaderValue = v.parse().unwrap();
            h.insert(name, val);
        }
        h
    }

    #[test]
    fn same_origin_via_host_allowed() {
        // localhost and any other host both pass when Origin == Host.
        let h = headers(&[("host", "localhost:3000")]);
        assert!(origin_allowed("http://localhost:3000", &h, &[]));
        let h = headers(&[("host", "dashboard.local:3000")]);
        assert!(origin_allowed("http://dashboard.local:3000", &h, &[]));
    }

    #[test]
    fn same_origin_via_forwarded_host_allowed() {
        // Behind a proxy that rewrites Host: X-Forwarded-Host carries the real host.
        let h = headers(&[
            ("host", "localhost:8080"),
            ("x-forwarded-host", "dashboard.local:3000"),
        ]);
        assert!(origin_allowed("http://dashboard.local:3000", &h, &[]));
    }

    #[test]
    fn cross_origin_blocked() {
        let h = headers(&[("host", "dashboard.local:3000")]);
        assert!(!origin_allowed("http://evil.example.com", &h, &[]));
    }

    #[test]
    fn allowlisted_cross_origin_allowed() {
        let h = headers(&[("host", "localhost:8080")]);
        let allowed = vec!["https://ci.example.com".to_string()];
        assert!(origin_allowed("https://ci.example.com", &h, &allowed));
        assert!(!origin_allowed("https://other.example.com", &h, &allowed));
    }

    #[test]
    fn no_host_header_blocks_nonlisted_origin() {
        let h = headers(&[]);
        assert!(!origin_allowed("http://localhost:3000", &h, &[]));
    }
}
