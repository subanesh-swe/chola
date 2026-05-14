use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::query;
use crate::state::ControllerState;
use crate::storage::{AnalyticsFilters, AnalyticsWindow, Granularity};

use super::date_parse::parse_flexible_datetime;
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
    /// `auto` (default), `hour`, or `day`. Auto picks hour when range <= 7d.
    pub granularity: Option<String>,
    /// ChQL expression for free-form filtering. Layered on top of typed
    /// filters when both are present. See `local/plans/CHQL.md`.
    pub q: Option<String>,
}

/// Threshold (inclusive) under which `auto` granularity picks hourly bucketing.
const AUTO_HOUR_MAX_DAYS: i64 = 7;

/// Pick effective bucket size from user mode + resolved window.
/// - `Some("hour")` → Hour (forced)
/// - `Some("day")`  → Day  (forced)
/// - `None` / `Some("auto")` → Hour when `to - from <= 7 days`, else Day
///
/// Unknown strings fall through to auto.
pub(crate) fn resolve_granularity(
    mode: Option<&str>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Granularity {
    match mode.map(str::to_ascii_lowercase).as_deref() {
        Some("hour") => Granularity::Hour,
        Some("day") => Granularity::Day,
        _ => auto_granularity(from, to),
    }
}

/// Auto bucket: hour if span <= 7 days, day otherwise. Negative/zero spans
/// fall back to hour (no buckets either way).
pub(crate) fn auto_granularity(from: DateTime<Utc>, to: DateTime<Utc>) -> Granularity {
    let span = to.signed_duration_since(from);
    if span <= Duration::days(AUTO_HOUR_MAX_DAYS) {
        Granularity::Hour
    } else {
        Granularity::Day
    }
}

/// Expand `AnalyticsWindow` into concrete `(from, to)` for granularity decisions.
/// `LastDays(n)` is anchored at `now`.
fn window_bounds(window: &AnalyticsWindow, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    match window {
        AnalyticsWindow::Range { from, to } => (*from, *to),
        AnalyticsWindow::LastDays(days) => (now - Duration::days(*days as i64), now),
    }
}

/// Parse + compile an optional `?q=` ChQL expression to a SqlFragment. Empty
/// or whitespace-only input yields `None` so the storage path stays fast.
pub(crate) fn compile_chql_param(q: Option<&str>) -> Result<Option<query::SqlFragment>, ApiError> {
    let Some(raw) = q else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let ast = query::parse(raw).map_err(ApiError::from)?;
    let Some(ast) = ast else {
        return Ok(None);
    };
    let frag = query::compile(&ast).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Some(frag))
}

/// Build an AnalyticsFilters from query params. `from`/`to` win over `days`.
fn filters_from_params(params: &AnalyticsParams) -> Result<AnalyticsFilters, ApiError> {
    let window = match (params.from.as_deref(), params.to.as_deref()) {
        (Some(f), Some(t)) => AnalyticsWindow::Range {
            from: parse_flexible_datetime("from", f, false)?,
            to: parse_flexible_datetime("to", t, true)?,
        },
        (Some(f), None) => AnalyticsWindow::Range {
            from: parse_flexible_datetime("from", f, false)?,
            to: Utc::now(),
        },
        (None, Some(t)) => {
            // Without a `from`, fall back to `days` window ending at `to`.
            let to = parse_flexible_datetime("to", t, true)?;
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
    let (from_eff, to_eff) = window_bounds(&window, Utc::now());
    let granularity = resolve_granularity(params.granularity.as_deref(), from_eff, to_eff);
    Ok(AnalyticsFilters {
        window,
        repo_id: params.repo_id,
        branch,
        stage_name,
        exit_code: params.exit_code,
        granularity,
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
        ("granularity" = Option<String>, Query, description = "Bucket size: auto (default), hour, day"),
        ("q" = Option<String>, Query, description = "ChQL expression layered over typed filters"),
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
    let chql_frag = compile_chql_param(params.q.as_deref())?;
    let chql_ref = chql_frag.as_ref();

    let (build_trends, duration_trends, slowest_stages, failing_repos, worker_util, queue_wait) =
        tokio::try_join!(
            storage.get_build_trends(&filters, chql_ref),
            storage.get_duration_trends(&filters, chql_ref),
            storage.get_slowest_stages(&filters, 10, chql_ref),
            storage.get_most_failing_repos(&filters, 10, chql_ref),
            storage.get_worker_utilization(),
            storage.get_queue_wait_trends(&filters, chql_ref),
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

#[cfg(test)]
mod tests {
    use super::{auto_granularity, resolve_granularity, Granularity};
    use chrono::{Duration, TimeZone, Utc};

    fn anchor() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap()
    }

    #[test]
    fn auto_one_day_picks_hour() {
        let to = anchor();
        let from = to - Duration::days(1);
        assert_eq!(auto_granularity(from, to), Granularity::Hour);
    }

    #[test]
    fn auto_seven_days_picks_hour() {
        let to = anchor();
        let from = to - Duration::days(7);
        assert_eq!(auto_granularity(from, to), Granularity::Hour);
    }

    #[test]
    fn auto_eight_days_picks_day() {
        let to = anchor();
        let from = to - Duration::days(8);
        assert_eq!(auto_granularity(from, to), Granularity::Day);
    }

    #[test]
    fn auto_sixty_days_picks_day() {
        let to = anchor();
        let from = to - Duration::days(60);
        assert_eq!(auto_granularity(from, to), Granularity::Day);
    }

    #[test]
    fn auto_just_over_seven_days_picks_day() {
        let to = anchor();
        let from = to - Duration::days(7) - Duration::seconds(1);
        assert_eq!(auto_granularity(from, to), Granularity::Day);
    }

    #[test]
    fn explicit_hour_overrides_long_range() {
        let to = anchor();
        let from = to - Duration::days(90);
        assert_eq!(
            resolve_granularity(Some("hour"), from, to),
            Granularity::Hour
        );
    }

    #[test]
    fn explicit_day_overrides_short_range() {
        let to = anchor();
        let from = to - Duration::days(1);
        assert_eq!(
            resolve_granularity(Some("day"), from, to),
            Granularity::Day
        );
    }

    #[test]
    fn auto_mode_string_matches_default() {
        let to = anchor();
        let from = to - Duration::days(3);
        assert_eq!(
            resolve_granularity(Some("auto"), from, to),
            Granularity::Hour
        );
    }

    #[test]
    fn unknown_mode_falls_back_to_auto() {
        let to = anchor();
        let from = to - Duration::days(30);
        assert_eq!(
            resolve_granularity(Some("week"), from, to),
            Granularity::Day
        );
    }

    #[test]
    fn case_insensitive_explicit_mode() {
        let to = anchor();
        let from = to - Duration::days(30);
        assert_eq!(
            resolve_granularity(Some("HOUR"), from, to),
            Granularity::Hour
        );
    }
}
