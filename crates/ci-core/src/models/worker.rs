use serde::{Deserialize, Serialize};

/// Disk type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskType {
    Nvme,
    Sata,
}

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskType::Nvme => write!(f, "nvme"),
            DiskType::Sata => write!(f, "sata"),
        }
    }
}

/// Worker connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerStatus {
    Connected,
    Disconnected,
    Draining,
}

/// Static worker information (sent at registration)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub hostname: String,
    pub total_cpu: u32,
    pub total_memory_mb: u64,
    pub total_disk_mb: u64,
    pub disk_type: DiskType,
    pub supported_job_types: Vec<String>,
    pub docker_enabled: bool,
}

/// Dynamic worker resource snapshot (sent via heartbeat)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHeartbeat {
    pub worker_id: String,
    pub used_cpu_percent: f64,
    pub used_memory_mb: u64,
    pub used_disk_mb: u64,
    pub running_job_ids: Vec<String>,
    pub system_load: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Full worker state as tracked by the controller
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    pub info: WorkerInfo,
    pub status: WorkerStatus,
    pub last_heartbeat: Option<WorkerHeartbeat>,
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

impl WorkerState {
    pub fn new(info: WorkerInfo) -> Self {
        Self {
            info,
            status: WorkerStatus::Connected,
            last_heartbeat: None,
            registered_at: chrono::Utc::now(),
        }
    }

    /// Available memory in MB
    pub fn free_memory_mb(&self) -> u64 {
        match &self.last_heartbeat {
            Some(hb) => self.info.total_memory_mb.saturating_sub(hb.used_memory_mb),
            None => self.info.total_memory_mb,
        }
    }

    /// Available disk in MB
    pub fn free_disk_mb(&self) -> u64 {
        match &self.last_heartbeat {
            Some(hb) => self.info.total_disk_mb.saturating_sub(hb.used_disk_mb),
            None => self.info.total_disk_mb,
        }
    }
}
