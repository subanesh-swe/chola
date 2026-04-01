pub mod audit;
pub mod auth_handlers;
pub mod dashboard_handlers;
pub mod error;
pub mod job_group_handlers;
pub mod job_handlers;
pub mod log_handlers;
pub mod repo_handlers;
pub mod script_handlers;
pub mod user_handlers;
pub mod webhook_handlers;
pub mod worker_handlers;

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};

use crate::rate_limit::{extract_ip, RateLimiter};
use crate::state::ControllerState;

/// Build the full REST API router (nested under /api/v1 by the caller).
pub fn api_router() -> Router<Arc<ControllerState>> {
    let login_limiter = Arc::new(RateLimiter::new(5, Duration::from_secs(60)));
    let webhook_limiter = Arc::new(RateLimiter::new(60, Duration::from_secs(60)));

    Router::new()
        // Auth (login is public, me/logout need auth)
        .route(
            "/auth/login",
            post(auth_handlers::login).route_layer(middleware::from_fn(
                move |req: Request<Body>, next: Next| {
                    let l = login_limiter.clone();
                    async move {
                        if l.check(extract_ip(&req)) {
                            next.run(req).await
                        } else {
                            StatusCode::TOO_MANY_REQUESTS.into_response()
                        }
                    }
                },
            )),
        )
        .route("/auth/me", get(auth_handlers::me))
        .route("/auth/logout", post(auth_handlers::logout))
        // Users
        .route(
            "/users",
            get(user_handlers::list).post(user_handlers::create),
        )
        .route(
            "/users/{id}",
            get(user_handlers::get_one)
                .put(user_handlers::update)
                .delete(user_handlers::delete_one),
        )
        // Repos
        .route(
            "/repos",
            get(repo_handlers::list).post(repo_handlers::create),
        )
        .route(
            "/repos/{id}",
            get(repo_handlers::get_one)
                .put(repo_handlers::update)
                .delete(repo_handlers::delete_one),
        )
        .route(
            "/repos/{id}/stages",
            get(repo_handlers::list_stages).post(repo_handlers::create_stage),
        )
        .route(
            "/repos/{repo_id}/stages/{stage_id}",
            put(repo_handlers::update_stage).delete(repo_handlers::delete_stage),
        )
        // Stage scripts
        .route(
            "/repos/{repo_id}/stages/{stage_id}/scripts",
            get(script_handlers::list).post(script_handlers::create),
        )
        .route(
            "/repos/{repo_id}/stages/{stage_id}/scripts/{script_id}",
            put(script_handlers::update).delete(script_handlers::delete_one),
        )
        // Webhooks — public receive endpoint (rate-limited), CRUD under /repos
        .route(
            "/webhooks/{provider}/{secret}",
            post(webhook_handlers::receive).route_layer(middleware::from_fn(
                move |req: Request<Body>, next: Next| {
                    let l = webhook_limiter.clone();
                    async move {
                        if l.check(extract_ip(&req)) {
                            next.run(req).await
                        } else {
                            StatusCode::TOO_MANY_REQUESTS.into_response()
                        }
                    }
                },
            )),
        )
        .route(
            "/repos/{id}/webhooks",
            get(webhook_handlers::list_for_repo).post(webhook_handlers::create),
        )
        .route(
            "/repos/{repo_id}/webhooks/{webhook_id}",
            delete(webhook_handlers::delete),
        )
        // Job groups
        .route(
            "/job-groups",
            get(job_group_handlers::list).post(job_group_handlers::trigger),
        )
        .route("/job-groups/{id}", get(job_group_handlers::get_one))
        .route("/job-groups/{id}/cancel", post(job_group_handlers::cancel))
        .route("/job-groups/{id}/retry", post(job_group_handlers::retry))
        .route("/job-groups/{id}/jobs", get(job_handlers::list_by_group))
        // Jobs
        .route("/jobs/{id}", get(job_handlers::get_one))
        .route("/jobs/{id}/cancel", post(job_handlers::cancel))
        .route("/jobs/{id}/logs", get(log_handlers::get_logs))
        .route("/jobs/{id}/logs/stream", get(log_handlers::stream_logs))
        // Workers
        .route("/workers", get(worker_handlers::list))
        .route("/workers/{id}", get(worker_handlers::get_one))
        .route("/workers/{id}/drain", post(worker_handlers::drain))
        .route("/workers/{id}/undrain", post(worker_handlers::undrain))
        // Dashboard
        .route("/dashboard/summary", get(dashboard_handlers::summary))
}
