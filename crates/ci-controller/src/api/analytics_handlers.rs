use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;
use crate::storage::{AnalyticsFilters, AnalyticsWindow};

use super::error::ApiError;

#[derive(Debug, Deserialize)]
pub struct AnalyticsParams {
    pub days: Option<i32>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub repo_id: Option<Uuid>,
    pub branch: Option<String>,
    pub stage_name: Option<String>,
    /// `-1` means "any non-zero" (matches `list_job_groups_paginated` convention).
    pub exit_code: Option<i32>,
}

fn parse_rfc3339(field: &str, value: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| ApiError::BadRequest(format!("Invalid {field} (expected RFC3339): {e}")))
}

/// Build an AnalyticsFilters from query params. `from`/`to` win over `days`.
fn filters_from_params(params: &AnalyticsParams) -> Result<AnalyticsFilters, ApiError> {
    let window = match (params.from.as_deref(), params.to.as_deref()) {
        (Some(f), Some(t)) => AnalyticsWindow::Range {
            from: parse_rfc3339("from", f)?,
            to: parse_rfc3339("to", t)?,
        },
        (Some(f), None) => AnalyticsWindow::Range {
            from: parse_rfc3339("from", f)?,
            to: Utc::now(),
        },
        (None, Some(t)) => {
            // Without a `from`, fall back to `days` window ending at `to`.
            let to = parse_rfc3339("to", t)?;
            let days = params.days.unwrap_or(30).clamp(1, 365);
            let from = to - chrono::Duration::days(days as i64);
            AnalyticsWindow::Range { from, to }
        }
        (None, None) => AnalyticsWindow::LastDays(params.days.unwrap_or(30).clamp(1, 365)),
    };
    let branch = params
        .branch
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let stage_name = params
        .stage_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(AnalyticsFilters {
        window,
        repo_id: params.repo_id,
        branch,
        stage_name,
        exit_code: params.exit_code,
    })
}

/// GET /api/v1/analytics
#[utoipa::path(
    get,
    path = "/api/v1/analytics",
    tag = "Analytics",
    params(
        ("days" = Option<i32>, Query, description = "Number of days (default 30); ignored if from/to set"),
        ("from" = Option<String>, Query, description = "Start RFC3339 (overrides days)"),
        ("to" = Option<String>, Query, description = "End RFC3339 (overrides days)"),
        ("repo_id" = Option<uuid::Uuid>, Query, description = "Filter by repo"),
        ("branch" = Option<String>, Query, description = "Filter by branch"),
        ("stage_name" = Option<String>, Query, description = "Filter by stage name"),
        ("exit_code" = Option<i32>, Query, description = "Filter by exit code; -1 = any non-zero"),
    ),
    responses(
        (status = 200, description = "Build analytics"),
        (status = 400, description = "Bad request"),
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
    let filters = filters_from_params(&params)?;

    let (build_trends, duration_trends, slowest_stages, failing_repos, worker_util, queue_wait) =
        tokio::try_join!(
            storage.get_build_trends(&filters),
            storage.get_duration_trends(&filters),
            storage.get_slowest_stages(&filters, 10),
            storage.get_most_failing_repos(&filters, 10),
            storage.get_worker_utilization(),
            storage.get_queue_wait_trends(&filters),
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
