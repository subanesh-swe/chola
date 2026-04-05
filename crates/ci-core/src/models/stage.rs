use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Repository configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Repo {
    pub id: Uuid,
    pub repo_name: String,
    pub repo_url: String,
    pub default_branch: String,
    pub enabled: bool,
    pub max_concurrent_builds: i32,
    pub cancel_superseded: bool,
    pub global_pre_script: Option<String>,
    pub global_pre_script_scope: String,
    pub global_post_script: Option<String>,
    pub global_post_script_scope: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Stage configuration for a repo
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StageConfig {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub stage_name: String,
    pub command: Option<String>,
    pub required_cpu: i32,
    pub required_memory_mb: i32,
    pub required_disk_mb: i32,
    pub max_duration_secs: i32,
    pub execution_order: i32,
    pub parallel_group: Option<String>,
    pub allow_worker_migration: bool,
    pub job_type: String,
    pub depends_on: Vec<String>,
    pub required_labels: Vec<String>,
    pub max_retries: i32,
    /// Command mode: "fixed" (default), "optional" (user can override), "required" (user must provide)
    pub command_mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Pre/post scripts for a stage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StageScript {
    pub id: Uuid,
    pub stage_config_id: Uuid,
    pub worker_id: Option<String>,
    pub script_type: String,  // "pre" or "post"
    pub script_scope: String, // "worker" or "master"
    pub script: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Webhook configuration for a repo
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Webhook {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub provider: String,
    pub secret: String,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Worker reservation record
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerReservation {
    pub id: Uuid,
    pub worker_id: String,
    pub job_group_id: Uuid,
    pub reserved_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub release_reason: Option<String>,
}
