use serde::{Deserialize, Serialize};

// ============================================================================
// Controller Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub tls: Option<TlsServerConfig>,
    pub storage: StorageConfig,
    pub redis: RedisConfig,
    pub scheduling: SchedulingConfig,
    pub workers: WorkersConfig,
    #[serde(default)]
    pub jobs: JobsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsServerConfig {
    #[serde(default)]
    pub enabled: bool,
    pub ca_cert: Option<String>,
    pub server_cert: Option<String>,
    pub server_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub postgres: PostgresConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
}

fn default_key_prefix() -> String {
    "ci:".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub nvme_preference: bool,
    #[serde(default)]
    pub branch_affinity: bool,
}

fn default_strategy() -> String {
    "best-fit".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkersConfig {
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u32,
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u32,
    #[serde(default = "default_max_reconnect")]
    pub max_reconnect_attempts: u32,
}

fn default_heartbeat_interval() -> u32 {
    3
}
fn default_heartbeat_timeout() -> u32 {
    10
}
fn default_max_reconnect() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsConfig {
    #[serde(default = "default_orphan_timeout")]
    pub orphan_timeout_secs: u64,
}

fn default_orphan_timeout() -> u64 {
    300 // 5 minutes
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            orphan_timeout_secs: default_orphan_timeout(),
        }
    }
}

// ============================================================================
// Worker Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub hostname: String,
    pub controller: ControllerConnectionConfig,
    pub resources: ResourcesConfig,
    pub capabilities: CapabilitiesConfig,
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub reconnect: ReconnectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConnectionConfig {
    pub address: String,
    #[serde(default)]
    pub tls: Option<TlsClientConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsClientConfig {
    #[serde(default)]
    pub enabled: bool,
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesConfig {
    pub total_cpu: u32,
    pub total_memory_gb: u64,
    pub total_disk_gb: u64,
    #[serde(default = "default_disk_type")]
    pub disk_type: String,
}

fn default_disk_type() -> String {
    "sata".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesConfig {
    #[serde(default)]
    pub supported_job_types: Vec<String>,
    #[serde(default)]
    pub docker_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    #[serde(default = "default_heartbeat_interval")]
    pub interval_secs: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_heartbeat_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_work_dir")]
    pub work_dir: String,
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
}

fn default_work_dir() -> String {
    "/var/lib/ci-worker/jobs".to_string()
}
fn default_log_dir() -> String {
    "/var/lib/ci-worker/logs".to_string()
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            work_dir: default_work_dir(),
            log_dir: default_log_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    #[serde(default = "default_max_reconnect_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_initial_delay_ms")]
    pub initial_delay_ms: u64,
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,
}

fn default_max_reconnect_attempts() -> u32 {
    5
}
fn default_initial_delay_ms() -> u64 {
    1000
}
fn default_max_delay_ms() -> u64 {
    30000
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_reconnect_attempts(),
            initial_delay_ms: default_initial_delay_ms(),
            max_delay_ms: default_max_delay_ms(),
        }
    }
}

// ============================================================================
// Shared
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub log_dir: Option<String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            log_dir: None,
        }
    }
}

// ============================================================================
// Config Loading
// ============================================================================

impl ControllerConfig {
    pub fn from_file(path: &str) -> crate::errors::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

impl WorkerConfig {
    pub fn from_file(path: &str) -> crate::errors::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
