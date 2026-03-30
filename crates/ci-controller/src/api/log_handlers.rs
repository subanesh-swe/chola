use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::middleware::AuthUser;
use crate::state::ControllerState;

use super::error::ApiError;

#[derive(Deserialize, Default)]
pub struct LogParams {
    pub from_offset: Option<u64>,
}

/// GET /api/v1/jobs/:id/logs -- return accumulated log data.
pub async fn get_logs(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let aggregator = state.log_aggregator.read().await;
    let data = aggregator.get_log_string(&job_id);
    let offset = aggregator.last_offset(&job_id);
    let complete = aggregator.is_complete(&job_id);

    Ok(Json(json!({
        "job_id": job_id,
        "data": data,
        "last_offset": offset,
        "complete": complete,
    })))
}

/// GET /api/v1/jobs/:id/logs/stream -- SSE stream of live log chunks.
///
/// Uses a channel-backed stream to avoid needing the `async-stream` crate.
pub async fn stream_logs(
    State(state): State<Arc<ControllerState>>,
    _auth_user: AuthUser,
    Path(job_id): Path<String>,
    Query(params): Query<LogParams>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let from_offset = params.from_offset.unwrap_or(0);

    let (buffered, mut live_rx, initially_complete) = {
        let mut aggregator = state.log_aggregator.write().await;
        aggregator.subscribe(&job_id, from_offset)
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);

    tokio::spawn(async move {
        // Send buffered data first
        for chunk in buffered {
            let payload = json!({
                "offset": chunk.offset,
                "data": String::from_utf8_lossy(&chunk.data),
            });
            if tx
                .send(Ok(Event::default().data(payload.to_string())))
                .await
                .is_err()
            {
                return;
            }
        }

        if initially_complete {
            let _ = tx
                .send(Ok(Event::default().event("complete").data("{}")))
                .await;
            return;
        }

        // Stream live updates
        loop {
            match live_rx.recv().await {
                Ok(chunk) => {
                    let payload = json!({
                        "offset": chunk.offset,
                        "data": String::from_utf8_lossy(&chunk.data),
                    });
                    if tx
                        .send(Ok(Event::default().data(payload.to_string())))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    let _ = tx
                        .send(Ok(Event::default().event("complete").data("{}")))
                        .await;
                    return;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Continue -- client will get next messages
                }
            }
        }
    });

    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
}
