use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Job state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobState {
    Queued,
    Assigned,
    Running,
    Success,
    Failed,
    Cancelled,
    Unknown,
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobState::Queued => write!(f, "QUEUED"),
            JobState::Assigned => write!(f, "ASSIGNED"),
            JobState::Running => write!(f, "RUNNING"),
            JobState::Success => write!(f, "SUCCESS"),
            JobState::Failed => write!(f, "FAILED"),
            JobState::Cancelled => write!(f, "CANCELLED"),
            JobState::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Job type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobType {
    Common,
    Heavy,
    Nix,
    Test,
}

impl std::fmt::Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::Common => write!(f, "common"),
            JobType::Heavy => write!(f, "heavy"),
            JobType::Nix => write!(f, "nix"),
            JobType::Test => write!(f, "test"),
        }
    }
}

/// Full job definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub job_id: String,
    pub command: String,
    pub job_type: JobType,
    pub required_cpu: u32,
    pub required_memory_mb: u64,
    pub required_disk_mb: u64,
    pub isolation_required: bool,
    pub preferred_worker: Option<String>,
    pub branch_id: Option<String>,
    pub environment: HashMap<String, String>,
    pub state: JobState,
    pub assigned_worker: Option<String>,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Connection ID of the submitter (e.g., peer addr of ci-job-runner)
    pub submitter_connection_id: Option<String>,
    /// Cancel reason if the job was cancelled
    pub cancel_reason: Option<String>,
}

impl Job {
    pub fn new(
        job_id: String,
        command: String,
        job_type: JobType,
        required_cpu: u32,
        required_memory_mb: u64,
        required_disk_mb: u64,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            job_id,
            command,
            job_type,
            required_cpu,
            required_memory_mb,
            required_disk_mb,
            isolation_required: false,
            preferred_worker: None,
            branch_id: None,
            environment: HashMap::new(),
            state: JobState::Queued,
            assigned_worker: None,
            exit_code: None,
            output: None,
            created_at: now,
            updated_at: now,
            submitter_connection_id: None,
            cancel_reason: None,
        }
    }
}
