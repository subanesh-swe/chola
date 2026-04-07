use serde::{Deserialize, Serialize};

// ============================================================================
// Controller Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub tls: Option<TlsServerConfig>,
    #[serde(default)]
    pub http_tls: Option<TlsServerConfig>,
    pub storage: StorageConfig,
    pub redis: RedisConfig,
    pub scheduling: SchedulingConfig,
    pub workers: WorkersConfig,
    #[serde(default)]
    pub jobs: JobsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default = "default_controller_http_port")]
    pub http_port: u16,
    #[serde(default)]
    pub retention: Option<RetentionConfig>,
}

fn default_controller_http_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub max_age_days: u32,
    pub max_builds_per_repo: u32,
    pub cleanup_interval_secs: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_age_days: 90,
            max_builds_per_repo: 500,
            cleanup_interval_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    #[serde(default = "default_jwt_expiry")]
    pub jwt_expiry_secs: u64,
    #[serde(default)]
    pub default_admin_username: Option<String>,
    #[serde(default)]
    pub default_admin_password: Option<String>,
    /// AES-256-GCM key for encrypting secret pipeline variables. If unset, plaintext stored.
    #[serde(default)]
    pub encryption_key: Option<String>,
}

fn default_jwt_secret() -> String {
    "change-me-in-production".to_string()
}

fn default_jwt_expiry() -> u64 {
    86400
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token: None,
            jwt_secret: default_jwt_secret(),
            jwt_expiry_secs: default_jwt_expiry(),
            default_admin_username: None,
            default_admin_password: None,
            encryption_key: None,
        }
    }
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
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_schema")]
    pub schema: String,
}

impl PostgresConfig {
    /// Build the connection URL. Env vars win over config file.
    /// Returns (url, Vec<(field, source)>) for logging by the caller.
    pub fn database_url(&self) -> (String, Vec<(&'static str, &'static str)>) {
        let mut sources = Vec::new();
        let host = env_or(&self.host, "CHOLA_DB_HOST", "host", &mut sources);
        let port = env_or(
            &self.port.to_string(),
            "CHOLA_DB_PORT",
            "port",
            &mut sources,
        );
        let database = env_or(&self.database, "CHOLA_DB_NAME", "database", &mut sources);
        let user = env_or(&self.user, "CHOLA_DB_USER", "user", &mut sources);
        let password = env_or(
            &self.password,
            "CHOLA_DB_PASSWORD",
            "password",
            &mut sources,
        );

        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            user, password, host, port, database
        );
        (url, sources)
    }
}

fn env_or(
    config_val: &str,
    env_key: &str,
    label: &'static str,
    sources: &mut Vec<(&'static str, &'static str)>,
) -> String {
    match std::env::var(env_key) {
        Ok(val) if !val.is_empty() => {
            sources.push((label, "env"));
            val
        }
        _ => {
            sources.push((label, "config"));
            config_val.to_string()
        }
    }
}

fn default_port() -> u16 {
    5432
}

fn default_max_connections() -> u32 {
    10
}

fn default_schema() -> String {
    "chola".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_redis_port")]
    pub port: u16,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
}

impl RedisConfig {
    /// Build the connection URL. Env vars win over config file.
    pub fn redis_url(&self) -> (String, Vec<(&'static str, &'static str)>) {
        let mut sources = Vec::new();
        let host = env_or(&self.host, "CHOLA_REDIS_HOST", "host", &mut sources);
        let port = env_or(
            &self.port.to_string(),
            "CHOLA_REDIS_PORT",
            "port",
            &mut sources,
        );
        let password = env_or(
            &self.password,
            "CHOLA_REDIS_PASSWORD",
            "password",
            &mut sources,
        );

        let url = if password.is_empty() {
            format!("redis://{}:{}", host, port)
        } else {
            format!("redis://:{}@{}:{}", password, host, port)
        };
        (url, sources)
    }
}

fn default_redis_port() -> u16 {
    6379
}

fn default_key_prefix() -> String {
    "chola:".to_string()
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
    #[serde(default = "default_reservation_timeout")]
    pub reservation_timeout_secs: u64,
    /// Seconds a group can stay in `Reserved` with no stage submitted before reaping.
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Seconds a group can stay in `Running` with no activity before reaping.
    #[serde(default = "default_stall_timeout")]
    pub stall_timeout_secs: u64,
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
fn default_reservation_timeout() -> u64 {
    14400 // 4 hours
}
fn default_idle_timeout() -> u64 {
    300 // 5 minutes — reserved but no stage submitted
}
fn default_stall_timeout() -> u64 {
    1800 // 30 minutes — running but no activity
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
    /// Auth token. Prefix determines type:
    ///   chola_wkr_ = worker, chola_svc_ = runner
    /// Env: CHOLA_TOKEN
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default = "default_worker_http_port")]
    pub http_port: u16,
    /// Disk mount points to track. If empty, auto-detect and dedup by device.
    #[serde(default)]
    pub tracked_disk_paths: Vec<String>,
}

fn default_worker_http_port() -> u16 {
    8081
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConnectionConfig {
    pub address: String,
    #[serde(default)]
    pub tls: Option<TlsClientConfig>,
    /// HTTP REST URL of the controller (e.g. "http://localhost:8080").
    /// If omitted, derived from gRPC address by replacing port with 8080.
    #[serde(default)]
    pub http_url: Option<String>,
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
    #[serde(default)]
    pub labels: Vec<String>,
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
    #[serde(default = "default_repos_dir")]
    pub repos_dir: String,
}

/// Resolve chola data directory: $CHOLA_HOME > $XDG_DATA_HOME/chola > ~/.local/share/chola
pub fn chola_data_dir(sub: &str) -> String {
    if let Ok(chola_home) = std::env::var("CHOLA_HOME") {
        return format!("{chola_home}/{sub}");
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return format!("{xdg}/chola/{sub}");
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/.local/share/chola/{sub}");
    }
    format!("/var/lib/chola/{sub}")
}
fn default_work_dir() -> String {
    chola_data_dir("worker/jobs")
}
fn default_log_dir() -> String {
    chola_data_dir("worker/logs")
}
fn default_repos_dir() -> String {
    chola_data_dir("worker/repos")
}
impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            work_dir: default_work_dir(),
            log_dir: default_log_dir(),
            repos_dir: default_repos_dir(),
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
    0 // 0 = unlimited retries (never give up on transient errors)
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
    #[allow(clippy::result_large_err)]
    pub fn from_file(path: &str) -> crate::errors::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}

impl WorkerConfig {
    #[allow(clippy::result_large_err)]
    pub fn from_file(path: &str) -> crate::errors::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
