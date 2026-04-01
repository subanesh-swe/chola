use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
};

use ci_core::models::job_group::JobGroupState;

use crate::state::ControllerState;

const BADGE_CACHE: &str = "no-cache, no-store, must-revalidate";

fn badge_svg(label: &str, color: &str) -> String {
    let label_w = 37_i32;
    let value_w = (label.len() as i32) * 7 + 10;
    let total_w = label_w + value_w;
    let text_x = label_w + 5;
    let font = "DejaVu Sans,sans-serif";
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_w}\" height=\"20\">\
<rect width=\"{label_w}\" height=\"20\" fill=\"#555\"/>\
<rect x=\"{label_w}\" width=\"{value_w}\" height=\"20\" fill=\"{color}\"/>\
<text x=\"4\" y=\"14\" fill=\"#fff\" font-family=\"{font}\" font-size=\"11\">build</text>\
<text x=\"{text_x}\" y=\"14\" fill=\"#fff\" font-family=\"{font}\" font-size=\"11\">{label}</text>\
</svg>"
    )
}

/// GET /api/v1/repos/{name}/badge.svg — public, no auth required
pub async fn repo_badge(
    State(state): State<Arc<ControllerState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(storage) = state.storage.as_ref() else {
        return badge_response(badge_svg("unknown", "#9f9f9f"));
    };

    let repo = match storage.get_repo_by_name(&name).await {
        Ok(Some(r)) => r,
        _ => return badge_response(badge_svg("unknown", "#9f9f9f")),
    };

    let group = match storage.get_latest_job_group_for_repo(repo.id).await {
        Ok(g) => g,
        _ => return badge_response(badge_svg("unknown", "#9f9f9f")),
    };

    let (label, color) = match group.map(|g| g.state) {
        Some(JobGroupState::Success) => ("passing", "#4c1"),
        Some(JobGroupState::Failed) => ("failing", "#e05d44"),
        Some(JobGroupState::Running) | Some(JobGroupState::Reserved) => ("running", "#007ec6"),
        _ => ("unknown", "#9f9f9f"),
    };

    badge_response(badge_svg(label, color))
}

fn badge_response(svg: String) -> (StatusCode, [(header::HeaderName, &'static str); 2], String) {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, BADGE_CACHE),
        ],
        svg,
    )
}
