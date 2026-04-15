pub mod analytics_handlers;
pub mod api_key_handlers;
pub mod artifact_handlers;
pub mod audit;
pub mod auth_handlers;
pub mod badge_handlers;
pub mod blacklist_handlers;
pub mod dashboard_handlers;
pub mod error;
pub mod job_group_handlers;
pub mod job_handlers;
pub mod label_group_handlers;
pub mod log_handlers;
pub mod notification_handlers;
pub mod repo_handlers;
pub mod resource_handlers;
pub mod schedule_handlers;
pub mod script_handlers;
pub mod settings_handlers;
pub mod user_handlers;
pub mod variable_handlers;
pub mod webhook_handlers;
pub mod worker_handlers;
pub mod worker_token_handlers;

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;

use axum::extract::DefaultBodyLimit;

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
                            (
                                StatusCode::TOO_MANY_REQUESTS,
                                Json(json!({"error": "Too many requests"})),
                            )
                                .into_response()
                        }
                    }
                },
            )),
        )
        .route("/auth/me", get(auth_handlers::me))
        .route("/auth/logout", post(auth_handlers::logout))
        .route("/auth/password", put(auth_handlers::change_password))
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
        .route("/users/{id}/password", put(user_handlers::reset_password))
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
        // Stage resource history + recommendations
        .route(
            "/repos/{repo_id}/stages/{stage_id}/resources",
            get(resource_handlers::get_resources),
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
                            (
                                StatusCode::TOO_MANY_REQUESTS,
                                Json(json!({"error": "Too many requests"})),
                            )
                                .into_response()
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
        .route(
            "/repos/{repo_id}/webhooks/{webhook_id}/deliveries",
            get(webhook_handlers::list_deliveries),
        )
        // Job groups
        .route(
            "/job-groups",
            get(job_group_handlers::list).post(job_group_handlers::trigger),
        )
        .route("/job-groups/{id}", get(job_group_handlers::get_one))
        .route("/job-groups/{id}/cancel", post(job_group_handlers::cancel))
        .route("/job-groups/{id}/retry", post(job_group_handlers::retry))
        .route("/job-groups/{id}/stages", get(job_group_handlers::stages))
        .route("/job-groups/{id}/jobs", get(job_handlers::list_by_group))
        // Runs (individual job executions with group+repo context)
        .route("/runs", get(job_handlers::list_runs))
        // Jobs
        .route("/jobs/{id}", get(job_handlers::get_one))
        .route("/jobs/{id}/cancel", post(job_handlers::cancel))
        .route("/jobs/{id}/retry", post(job_handlers::retry))
        .route("/jobs/{id}/logs", get(log_handlers::get_logs))
        .route("/jobs/{id}/logs/stream", get(log_handlers::stream_logs))
        // Workers
        .route("/workers", get(worker_handlers::list))
        .route("/workers/register", post(worker_handlers::register_worker))
        .route(
            "/workers/{id}/regenerate-token",
            post(worker_handlers::regenerate_token),
        )
        .route(
            "/workers/{id}",
            get(worker_handlers::get_one).delete(worker_handlers::delete_worker),
        )
        .route("/workers/{id}/drain", post(worker_handlers::drain))
        .route("/workers/{id}/undrain", post(worker_handlers::undrain))
        .route("/workers/{id}/approve", put(worker_handlers::approve))
        .route("/workers/{id}/reject", put(worker_handlers::reject))
        .route(
            "/workers/{id}/labels",
            get(worker_handlers::get_labels).put(worker_handlers::update_labels),
        )
        .route(
            "/workers/{id}/metadata",
            put(worker_handlers::update_metadata),
        )
        .route("/workers/{id}/limits", put(worker_handlers::update_limits))
        // Worker tokens
        .route(
            "/worker-tokens",
            get(worker_token_handlers::list).post(worker_token_handlers::create),
        )
        .route(
            "/worker-tokens/{id}/activate",
            put(worker_token_handlers::activate),
        )
        .route(
            "/worker-tokens/{id}/deactivate",
            put(worker_token_handlers::deactivate),
        )
        .route(
            "/worker-tokens/{id}",
            delete(worker_token_handlers::delete_one),
        )
        // Label groups
        .route(
            "/label-groups",
            get(label_group_handlers::list).post(label_group_handlers::create),
        )
        .route(
            "/label-groups/{id}",
            get(label_group_handlers::get_one)
                .put(label_group_handlers::update)
                .delete(label_group_handlers::delete_one),
        )
        // Dashboard
        .route("/dashboard/summary", get(dashboard_handlers::summary))
        // Analytics
        .route("/analytics", get(analytics_handlers::get_analytics))
        // Variables
        .route(
            "/repos/{id}/variables",
            get(variable_handlers::list).post(variable_handlers::create),
        )
        .route(
            "/repos/{repo_id}/variables/{var_id}",
            put(variable_handlers::update).delete(variable_handlers::delete_one),
        )
        // Schedules
        .route(
            "/repos/{id}/schedules",
            get(schedule_handlers::list).post(schedule_handlers::create),
        )
        .route(
            "/repos/{repo_id}/schedules/{schedule_id}",
            put(schedule_handlers::update).delete(schedule_handlers::delete_one),
        )
        // Notifications
        .route(
            "/repos/{id}/notifications",
            get(notification_handlers::list).post(notification_handlers::create),
        )
        .route(
            "/repos/{repo_id}/notifications/{nid}",
            put(notification_handlers::update).delete(notification_handlers::delete),
        )
        // Settings
        .route(
            "/settings",
            get(settings_handlers::get_settings).put(settings_handlers::update_setting),
        )
        .route("/settings/{key}", delete(settings_handlers::delete_setting))
        // Audit log
        .route("/audit-log", get(audit::list_audit_logs))
        // Blacklist
        .route(
            "/blacklist/commands",
            get(blacklist_handlers::list_command_blacklist)
                .post(blacklist_handlers::create_command_blacklist),
        )
        .route(
            "/blacklist/commands/{id}",
            put(blacklist_handlers::update_command_blacklist)
                .delete(blacklist_handlers::delete_command_blacklist),
        )
        .route(
            "/blacklist/branches",
            get(blacklist_handlers::list_branch_blacklist)
                .post(blacklist_handlers::create_branch_blacklist),
        )
        .route(
            "/blacklist/branches/{id}",
            put(blacklist_handlers::update_branch_blacklist)
                .delete(blacklist_handlers::delete_branch_blacklist),
        )
        // Artifacts
        .route(
            "/artifacts/{group_id}/{stage_name}",
            post(artifact_handlers::upload_artifact)
                .layer(DefaultBodyLimit::max(100 * 1024 * 1024)),
        )
        .route(
            "/artifacts/{group_id}",
            get(artifact_handlers::list_artifacts),
        )
        .route(
            "/artifacts/download/{artifact_id}",
            get(artifact_handlers::download_artifact),
        )
        // API keys
        .route(
            "/auth/api-keys",
            get(api_key_handlers::list).post(api_key_handlers::create),
        )
        .route("/auth/api-keys/{id}", delete(api_key_handlers::revoke))
}

/// Public routes that require no authentication (badge SVG).
pub fn public_api_router() -> Router<Arc<ControllerState>> {
    Router::new().route("/repos/{name}/badge.svg", get(badge_handlers::repo_badge))
}
