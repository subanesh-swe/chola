use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Job group state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JobGroupState {
    Pending,
    Reserved,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobGroupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Reserved => write!(f, "reserved"),
            Self::Running => write!(f, "running"),
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl JobGroupState {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "reserved" => Self::Reserved,
            "running" => Self::Running,
            "success" => Self::Success,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}

/// Resources allocated for a job group reservation
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ToSchema)]
pub struct AllocatedResources {
    pub cpu: u32,
    pub memory_mb: u64,
    pub disk_mb: u64,
}

/// A job group represents a multi-stage build pipeline
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobGroup {
    pub id: Uuid,
    pub repo_id: Option<Uuid>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub trigger_source: String,
    pub reserved_worker_id: Option<String>,
    pub state: JobGroupState,
    pub priority: i32,
    pub pr_number: Option<i32>,
    pub idempotency_key: Option<String>,
    /// Resources allocated on the worker for this group
    #[serde(default)]
    pub allocated_resources: AllocatedResources,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Tracks last meaningful event (stage submit, job status report) for reaper timeouts.
    /// Not persisted to DB — set to `now()` on creation, `updated_at` on recovery.
    #[serde(default = "chrono::Utc::now")]
    pub last_activity_at: DateTime<Utc>,
}

impl JobGroup {
    pub fn new(repo_id: Uuid, branch: Option<String>, commit_sha: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repo_id: Some(repo_id),
            branch,
            commit_sha,
            trigger_source: "jenkins".to_string(),
            reserved_worker_id: None,
            state: JobGroupState::Pending,
            priority: 0,
            pr_number: None,
            idempotency_key: None,
            allocated_resources: AllocatedResources::default(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            last_activity_at: now,
        }
    }

    pub fn new_with_id(
        id: Uuid,
        repo_id: Option<Uuid>,
        branch: Option<String>,
        commit_sha: Option<String>,
        trigger_source: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            repo_id,
            branch,
            commit_sha,
            trigger_source,
            reserved_worker_id: None,
            state: JobGroupState::Running,
            priority: 0,
            pr_number: None,
            idempotency_key: None,
            allocated_resources: AllocatedResources::default(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            last_activity_at: now,
        }
    }
}
