use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::header,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::error;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn validate_path_component(name: &str, value: &str) -> Result<(), ApiError> {
    if value.contains("..") || value.contains('/') || value.contains('\\') || value.contains('\0') {
        return Err(ApiError::BadRequest(format!("Invalid {}", name)));
    }
    Ok(())
}

fn log_dir(state: &ControllerState) -> Result<&str, ApiError> {
    state
        .config
        .logging
        .log_dir
        .as_deref()
        .ok_or_else(|| ApiError::Internal("Log directory not configured".into()))
}

// ── POST /api/v1/artifacts/:group_id/:stage_name ────────────────────────────

pub async fn upload_artifact(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path((group_id, stage_name)): Path<(Uuid, String)>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    validate_path_component("stage_name", &stage_name)?;
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let base = log_dir(&state)?;

    let mut uploaded = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart error: {e}")))?
    {
        let filename = field
            .file_name()
            .ok_or_else(|| ApiError::BadRequest("Missing filename".into()))?
            .to_owned();
        validate_path_component("filename", &filename)?;

        let dir = format!("{}/artifacts/{}/{}", base, group_id, stage_name);
        tokio::fs::create_dir_all(&dir).await.map_err(|e| {
            error!("Failed to create artifact dir: {e}");
            ApiError::Internal(e.to_string())
        })?;

        let file_path = format!("{}/{}", dir, filename);

        // Stream to disk chunk-by-chunk (no full in-memory load)
        let file = tokio::fs::File::create(&file_path).await.map_err(|e| {
            error!("Failed to create artifact file: {e}");
            ApiError::Internal(e.to_string())
        })?;
        let mut writer = tokio::io::BufWriter::new(file);
        let max_size: u64 = 100 * 1024 * 1024; // 100MB
        let mut size: u64 = 0;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| ApiError::BadRequest(format!("Upload read error: {e}")))?
        {
            size += chunk.len() as u64;
            if size > max_size {
                drop(writer); // release file handle before removal
                let _ = tokio::fs::remove_file(&file_path).await;
                return Err(ApiError::BadRequest("File exceeds 100MB limit".into()));
            }
            writer.write_all(&chunk).await.map_err(|e| {
                error!("Failed to write artifact chunk: {e}");
                ApiError::Internal(e.to_string())
            })?;
        }
        writer.flush().await.map_err(|e| {
            error!("Failed to flush artifact file: {e}");
            ApiError::Internal(e.to_string())
        })?;

        let content_type = mime_guess::from_path(&filename)
            .first_or_octet_stream()
            .to_string();

        let id = storage
            .insert_artifact(
                group_id,
                None,
                &stage_name,
                &filename,
                &file_path,
                size as i64,
                &content_type,
            )
            .await
            .map_err(|e| {
                error!("Failed to insert artifact record: {e}");
                ApiError::Internal(e.to_string())
            })?;

        uploaded.push(json!({
            "id": id.to_string(),
            "filename": filename,
            "size_bytes": size,
            "content_type": content_type,
        }));
    }

    Ok(Json(json!({ "uploaded": uploaded })))
}

// ── GET /api/v1/artifacts/:group_id ─────────────────────────────────────────

pub async fn list_artifacts(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(group_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let artifacts = storage
        .list_artifacts_for_group(group_id)
        .await
        .map_err(|e| {
            error!("Failed to list artifacts: {e}");
            ApiError::Internal(e.to_string())
        })?;
    Ok(Json(
        json!({ "artifacts": artifacts, "count": artifacts.len() }),
    ))
}

// ── GET /api/v1/artifacts/download/:artifact_id ─────────────────────────────

pub async fn download_artifact(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(artifact_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let storage = state.storage.as_ref().ok_or(ApiError::StorageUnavailable)?;
    let (file_path, filename, content_type) = storage
        .get_artifact(artifact_id)
        .await
        .map_err(|e| {
            error!("Failed to get artifact: {e}");
            ApiError::Internal(e.to_string())
        })?
        .ok_or_else(|| ApiError::NotFound("Artifact not found".into()))?;

    let file = tokio::fs::File::open(&file_path).await.map_err(|e| {
        error!("Failed to open artifact file: {e}");
        ApiError::Internal(e.to_string())
    })?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        body,
    ))
}
