use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::storage::Storage;

/// Dispatch notifications for a completed job group.
/// `event_type` is "on_success" or "on_failure".
pub async fn dispatch_notifications(
    storage: &Arc<Storage>,
    repo_id: Uuid,
    event_type: &str,
    payload: Value,
) {
    let configs = match storage
        .get_notification_configs_for_trigger(repo_id, event_type)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Failed to fetch notification configs for repo {}: {}",
                repo_id, e
            );
            return;
        }
    };

    if configs.is_empty() {
        return;
    }

    let client = reqwest::Client::new();
    for cfg in &configs {
        let result = match cfg.channel_type.as_str() {
            "slack" => dispatch_slack(&client, &cfg.config, &payload).await,
            "webhook" => dispatch_webhook(&client, &cfg.config, &payload).await,
            other => {
                warn!(
                    "Unknown channel_type '{}' for notification {}",
                    other, cfg.id
                );
                continue;
            }
        };
        match result {
            Ok(_) => info!(
                "Notification {} dispatched (channel={})",
                cfg.id, cfg.channel_type
            ),
            Err(e) => error!(
                "Notification {} dispatch failed (channel={}): {}",
                cfg.id, cfg.channel_type, e
            ),
        }
    }
}

async fn dispatch_slack(
    client: &reqwest::Client,
    config: &Value,
    payload: &Value,
) -> anyhow::Result<()> {
    let url = config
        .get("webhook_url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("slack config missing webhook_url"))?;

    let body = build_slack_body(payload);
    client.post(url).json(&body).send().await?;
    Ok(())
}

async fn dispatch_webhook(
    client: &reqwest::Client,
    config: &Value,
    payload: &Value,
) -> anyhow::Result<()> {
    let url = config
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("webhook config missing url"))?;

    client.post(url).json(payload).send().await?;
    Ok(())
}

fn build_slack_body(payload: &Value) -> Value {
    let text = format!(
        "Build *{}* for `{}` branch `{}` — *{}*",
        payload
            .get("group_id")
            .and_then(Value::as_str)
            .unwrap_or("?"),
        payload.get("repo").and_then(Value::as_str).unwrap_or("?"),
        payload.get("branch").and_then(Value::as_str).unwrap_or("?"),
        payload.get("state").and_then(Value::as_str).unwrap_or("?"),
    );
    json!({ "text": text })
}
