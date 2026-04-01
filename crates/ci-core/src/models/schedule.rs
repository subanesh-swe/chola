use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Scheduled/cron build configuration for a repo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub interval_secs: i64,
    pub next_run_at: DateTime<Utc>,
    pub stages: Vec<String>,
    pub branch: String,
    pub enabled: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
