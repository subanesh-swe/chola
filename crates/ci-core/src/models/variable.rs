use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A repo-level environment variable injected into pipeline jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineVariable {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub name: String,
    pub value: String,
    pub is_secret: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
