use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Per-disk/partition detail reported by workers
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiskDetailInfo {
    pub mount_point: String,
    pub device: String,
    pub fs_type: String,
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
}

/// Disk type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerStatus {
    Connected,
    Disconnected,
    Draining,
}

/// Static worker information (sent at registration)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub hostname: String,
    pub total_cpu: u32,
    pub total_memory_mb: u64,
    pub total_disk_mb: u64,
    pub disk_type: DiskType,
    pub supported_job_types: Vec<String>,
    pub docker_enabled: bool,
    pub labels: Vec<String>,
    pub disk_details: Vec<DiskDetailInfo>,
}

/// Dynamic worker resource snapshot (sent via heartbeat)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerHeartbeat {
    pub worker_id: String,
    pub used_cpu_percent: f64,
    pub used_memory_mb: u64,
    pub used_disk_mb: u64,
    pub running_job_ids: Vec<String>,
    pub system_load: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub disk_details: Vec<DiskDetailInfo>,
}

/// Full worker state as tracked by the controller
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerState {
    pub info: WorkerInfo,
    pub status: WorkerStatus,
    pub last_heartbeat: Option<WorkerHeartbeat>,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    /// OS/kernel/arch metadata reported by the worker after registration
    #[serde(default)]
    pub system_info: Option<serde_json::Value>,
    /// CPU cores allocated to active builds
    #[serde(default)]
    pub allocated_cpu: u32,
    /// Memory (MB) allocated to active builds
    #[serde(default)]
    pub allocated_memory_mb: u64,
    /// Disk (MB) allocated to active builds
    #[serde(default)]
    pub allocated_disk_mb: u64,
}

impl WorkerState {
    pub fn new(info: WorkerInfo) -> Self {
        Self {
            info,
            status: WorkerStatus::Connected,
            last_heartbeat: None,
            registered_at: chrono::Utc::now(),
            system_info: None,
            allocated_cpu: 0,
            allocated_memory_mb: 0,
            allocated_disk_mb: 0,
        }
    }

    /// Try to allocate resources. Returns false if insufficient capacity.
    /// CPU/memory: checked against total - allocated (reservation model).
    /// Disk: checked against total - actual usage from heartbeat (usage model),
    /// because disk consumption persists after jobs complete.
    pub fn allocate(&mut self, cpu: u32, mem: u64, disk: u64) -> bool {
        if self.allocated_cpu + cpu > self.info.total_cpu
            || self.allocated_memory_mb + mem > self.info.total_memory_mb
        {
            return false;
        }
        // Disk uses actual free space, not reservation-based available
        if disk > 0 && self.free_disk_mb() < disk {
            return false;
        }
        self.allocated_cpu += cpu;
        self.allocated_memory_mb += mem;
        self.allocated_disk_mb += disk;
        true
    }

    /// Release previously allocated resources.
    pub fn release(&mut self, cpu: u32, mem: u64, disk: u64) {
        self.allocated_cpu = self.allocated_cpu.saturating_sub(cpu);
        self.allocated_memory_mb = self.allocated_memory_mb.saturating_sub(mem);
        self.allocated_disk_mb = self.allocated_disk_mb.saturating_sub(disk);
    }

    pub fn available_cpu(&self) -> u32 {
        self.info.total_cpu.saturating_sub(self.allocated_cpu)
    }

    pub fn available_memory_mb(&self) -> u64 {
        self.info
            .total_memory_mb
            .saturating_sub(self.allocated_memory_mb)
    }

    pub fn available_disk_mb(&self) -> u64 {
        self.info
            .total_disk_mb
            .saturating_sub(self.allocated_disk_mb)
    }

    /// Available memory in MB (from heartbeat)
    pub fn free_memory_mb(&self) -> u64 {
        match &self.last_heartbeat {
            Some(hb) => self.info.total_memory_mb.saturating_sub(hb.used_memory_mb),
            None => self.info.total_memory_mb,
        }
    }

    /// Available disk in MB (from heartbeat)
    pub fn free_disk_mb(&self) -> u64 {
        match &self.last_heartbeat {
            Some(hb) => self.info.total_disk_mb.saturating_sub(hb.used_disk_mb),
            None => self.info.total_disk_mb,
        }
    }
}
