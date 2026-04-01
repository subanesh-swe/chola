use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize)]
pub struct AnalyticsParams {
    pub days: Option<i32>,
}

/// GET /api/v1/analytics
#[utoipa::path(
    get,
    path = "/api/v1/analytics",
    tag = "Analytics",
    params(
        ("days" = Option<i32>, Query, description = "Number of days (default 30)"),
    ),
    responses(
        (status = 200, description = "Build analytics"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_analytics(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Query(params): Query<AnalyticsParams>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let days = params.days.unwrap_or(30).clamp(1, 365);

    let (build_trends, duration_trends, slowest_stages, failing_repos, worker_util, queue_wait) =
        tokio::try_join!(
            storage.get_build_trends(days),
            storage.get_duration_trends(days),
            storage.get_slowest_stages(days, 10),
            storage.get_most_failing_repos(days, 10),
            storage.get_worker_utilization(),
            storage.get_queue_wait_trends(days),
        )
        .map_err(|e| ApiError::Internal(format!("Analytics query failed: {}", e)))?;

    let total_builds: i64 = build_trends.iter().map(|p| p.total).sum();
    let total_success: i64 = build_trends.iter().map(|p| p.success).sum();
    let success_rate = if total_builds > 0 {
        (total_success as f64 / total_builds as f64 * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let avg_duration = if !duration_trends.is_empty() {
        duration_trends
            .iter()
            .map(|p| p.avg_duration_secs)
            .sum::<i64>()
            / duration_trends.len() as i64
    } else {
        0
    };
    let avg_wait = if !queue_wait.is_empty() {
        queue_wait.iter().map(|p| p.avg_wait_secs).sum::<i64>() / queue_wait.len() as i64
    } else {
        0
    };

    Ok(Json(json!({
        "summary": {
            "total_builds": total_builds,
            "success_rate": success_rate,
            "avg_duration_secs": avg_duration,
            "avg_queue_wait_secs": avg_wait,
        },
        "build_trends": build_trends,
        "duration_trends": duration_trends,
        "slowest_stages": slowest_stages,
        "failing_repos": failing_repos,
        "worker_utilization": worker_util,
        "queue_wait_trends": queue_wait,
    })))
}
