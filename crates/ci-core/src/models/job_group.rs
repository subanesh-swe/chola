use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Job group state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// A job group represents a multi-stage build pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobGroup {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub trigger_source: String,
    pub reserved_worker_id: Option<String>,
    pub state: JobGroupState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl JobGroup {
    pub fn new(repo_id: Uuid, branch: Option<String>, commit_sha: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repo_id,
            branch,
            commit_sha,
            trigger_source: "jenkins".to_string(),
            reserved_worker_id: None,
            state: JobGroupState::Pending,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }
}
