use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Job state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Assigned,
    Running,
    Success,
    Failed,
    Cancelled,
    Unknown,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobState::Queued => write!(f, "queued"),
            JobState::Assigned => write!(f, "assigned"),
            JobState::Running => write!(f, "running"),
            JobState::Success => write!(f, "success"),
            JobState::Failed => write!(f, "failed"),
            JobState::Cancelled => write!(f, "cancelled"),
            JobState::Unknown => write!(f, "unknown"),
        }
    }
}

impl JobState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "assigned" => Self::Assigned,
            "running" => Self::Running,
            "success" => Self::Success,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Unknown,
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
    /// Job group this job belongs to (None for legacy single jobs)
    pub job_group_id: Option<uuid::Uuid>,
    /// Stage config ID from the database
    pub stage_config_id: Option<uuid::Uuid>,
    /// Stage name (e.g., "build", "test", "push-docker-image")
    pub stage_name: Option<String>,
    /// Pre-script to run before the main command
    pub pre_script: Option<String>,
    /// Post-script to run after the main command (MUST run even on abort)
    pub post_script: Option<String>,
    /// Pre-script exit code
    pub pre_exit_code: Option<i32>,
    /// Post-script exit code
    pub post_exit_code: Option<i32>,
    /// Maximum duration in seconds before timeout
    pub max_duration_secs: Option<i32>,
    /// Log file path
    pub log_path: Option<String>,
    /// When the job started running
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the job completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
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
            job_group_id: None,
            stage_config_id: None,
            stage_name: None,
            pre_script: None,
            post_script: None,
            pre_exit_code: None,
            post_exit_code: None,
            max_duration_secs: None,
            log_path: None,
            started_at: None,
            completed_at: None,
        }
    }
}
