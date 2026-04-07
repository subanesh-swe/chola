use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Acquire, Executor, PgPool, Row};
use tracing::info;
use uuid::Uuid;

use ci_core::models::api_key::ApiKey;
use ci_core::models::job_group::{JobGroup, JobGroupState};
use ci_core::models::schedule::CronSchedule;
use ci_core::models::stage::{Repo, StageConfig, StageScript, WorkerReservation};
use ci_core::models::user::{User, UserRole};
use ci_core::models::variable::PipelineVariable;

// ============================================================================
// Column list constants (prevent drift between SELECT / INSERT / RETURNING)
// ============================================================================

const REPO_COLUMNS: &str = "id, repo_name, repo_url, default_branch, enabled, \
     COALESCE(max_concurrent_builds, 0) AS max_concurrent_builds, \
     COALESCE(cancel_superseded, false) AS cancel_superseded, \
     global_pre_script, \
     COALESCE(global_pre_script_scope, 'worker') AS global_pre_script_scope, \
     global_post_script, \
     COALESCE(global_post_script_scope, 'worker') AS global_post_script_scope, \
     created_at, updated_at";

const STAGE_CONFIG_COLUMNS: &str =
    "id, repo_id, stage_name, command, required_cpu, required_memory_mb, \
     required_disk_mb, max_duration_secs, execution_order, parallel_group, \
     allow_worker_migration, job_type, depends_on, required_labels, max_retries, \
     command_mode, created_at, updated_at";

const STAGE_SCRIPT_COLUMNS: &str =
    "id, stage_config_id, worker_id, script_type, script_scope, script, \
     created_at, updated_at";

const JOB_GROUP_COLUMNS: &str =
    "id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id, \
     state, priority, pr_number, idempotency_key, \
     allocated_cpu, allocated_memory_mb, allocated_disk_mb, \
     created_at, updated_at, completed_at";

const JOB_COLUMNS: &str = "id, job_group_id, stage_config_id, stage_name, command, pre_script, \
     post_script, worker_id, state, exit_code, pre_exit_code, post_exit_code, \
     log_path, started_at, completed_at, retry_count, created_at, updated_at";

const WORKER_COLUMNS: &str =
    "worker_id, hostname, total_cpu, total_memory_mb, total_disk_mb, disk_type, \
     supported_job_types, docker_enabled, status, last_heartbeat_at, registered_at, labels, \
     system_info, worker_token_hash, registration_token_id, approved, description";

const RESERVATION_COLUMNS: &str =
    "id, worker_id, job_group_id, reserved_at, expires_at, released_at, release_reason";

const USER_COLUMNS: &str =
    "id, username, password_hash, display_name, role, active, created_at, updated_at";

const API_KEY_COLUMNS: &str = "id, user_id, name, created_at, last_used_at, revoked";

// ============================================================================
// Row mapping helpers
// ============================================================================

fn map_repo(r: sqlx::postgres::PgRow) -> Repo {
    Repo {
        id: r.get("id"),
        repo_name: r.get("repo_name"),
        repo_url: r.get("repo_url"),
        default_branch: r.get("default_branch"),
        enabled: r.get("enabled"),
        max_concurrent_builds: r.get("max_concurrent_builds"),
        cancel_superseded: r.get("cancel_superseded"),
        global_pre_script: r.get("global_pre_script"),
        global_pre_script_scope: r.get("global_pre_script_scope"),
        global_post_script: r.get("global_post_script"),
        global_post_script_scope: r.get("global_post_script_scope"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_stage_config(r: sqlx::postgres::PgRow) -> StageConfig {
    StageConfig {
        id: r.get("id"),
        repo_id: r.get("repo_id"),
        stage_name: r.get("stage_name"),
        command: r.get("command"),
        required_cpu: r.get("required_cpu"),
        required_memory_mb: r.get("required_memory_mb"),
        required_disk_mb: r.get("required_disk_mb"),
        max_duration_secs: r.get("max_duration_secs"),
        execution_order: r.get("execution_order"),
        parallel_group: r.get("parallel_group"),
        allow_worker_migration: r.get("allow_worker_migration"),
        job_type: r.get("job_type"),
        depends_on: r.get("depends_on"),
        required_labels: r.get("required_labels"),
        max_retries: r.get("max_retries"),
        command_mode: r
            .try_get("command_mode")
            .unwrap_or_else(|_| "fixed".to_string()),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_stage_script(r: sqlx::postgres::PgRow) -> StageScript {
    StageScript {
        id: r.get("id"),
        stage_config_id: r.get("stage_config_id"),
        worker_id: r.get("worker_id"),
        script_type: r.get("script_type"),
        script_scope: r.get("script_scope"),
        script: r.get("script"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_reservation(r: sqlx::postgres::PgRow) -> WorkerReservation {
    WorkerReservation {
        id: r.get("id"),
        worker_id: r.get("worker_id"),
        job_group_id: r.get("job_group_id"),
        reserved_at: r.get("reserved_at"),
        expires_at: r.get("expires_at"),
        released_at: r.get("released_at"),
        release_reason: r.get("release_reason"),
    }
}

fn map_job_group(r: sqlx::postgres::PgRow) -> JobGroup {
    let state_str: String = r.get("state");
    let updated_at: chrono::DateTime<chrono::Utc> = r.get("updated_at");
    JobGroup {
        id: r.get("id"),
        repo_id: r.get("repo_id"),
        branch: r.get("branch"),
        commit_sha: r.get("commit_sha"),
        trigger_source: r.get("trigger_source"),
        reserved_worker_id: r.get("reserved_worker_id"),
        state: JobGroupState::from_str(&state_str),
        priority: r.get("priority"),
        pr_number: r.try_get("pr_number").ok().flatten(),
        idempotency_key: r.try_get("idempotency_key").ok().flatten(),
        allocated_resources: ci_core::models::job_group::AllocatedResources {
            cpu: r.try_get::<i32, _>("allocated_cpu").unwrap_or(0) as u32,
            memory_mb: r.try_get::<i64, _>("allocated_memory_mb").unwrap_or(0) as u64,
            disk_mb: r.try_get::<i64, _>("allocated_disk_mb").unwrap_or(0) as u64,
        },
        created_at: r.get("created_at"),
        updated_at,
        completed_at: r.get("completed_at"),
        // Not persisted — use updated_at as best approximation on recovery
        last_activity_at: updated_at,
    }
}

fn map_user(r: &sqlx::postgres::PgRow) -> User {
    User {
        id: r.get("id"),
        username: r.get("username"),
        password_hash: r.get("password_hash"),
        display_name: r.get("display_name"),
        role: UserRole::from_db_str(r.get::<String, _>("role").as_str()),
        active: r.get("active"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_api_key(r: &sqlx::postgres::PgRow) -> ApiKey {
    ApiKey {
        id: r.get("id"),
        user_id: r.get("user_id"),
        name: r.get("name"),
        created_at: r.get("created_at"),
        last_used_at: r.get("last_used_at"),
        revoked: r.get("revoked"),
    }
}

impl From<sqlx::postgres::PgRow> for DbJob {
    fn from(r: sqlx::postgres::PgRow) -> Self {
        Self {
            id: r.get("id"),
            job_group_id: r.get("job_group_id"),
            stage_config_id: r.get("stage_config_id"),
            stage_name: r.get("stage_name"),
            command: r.get("command"),
            pre_script: r.get("pre_script"),
            post_script: r.get("post_script"),
            worker_id: r.get("worker_id"),
            state: r.get("state"),
            exit_code: r.get("exit_code"),
            pre_exit_code: r.get("pre_exit_code"),
            post_exit_code: r.get("post_exit_code"),
            log_path: r.get("log_path"),
            started_at: r.get("started_at"),
            completed_at: r.get("completed_at"),
            retry_count: r.get("retry_count"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }
}

impl From<sqlx::postgres::PgRow> for WorkerRow {
    fn from(r: sqlx::postgres::PgRow) -> Self {
        Self {
            worker_id: r.get("worker_id"),
            hostname: r.get("hostname"),
            total_cpu: r.get("total_cpu"),
            total_memory_mb: r.get("total_memory_mb"),
            total_disk_mb: r.get("total_disk_mb"),
            disk_type: r.get("disk_type"),
            supported_job_types: r.get("supported_job_types"),
            docker_enabled: r.get("docker_enabled"),
            status: r.get("status"),
            last_heartbeat_at: r.get("last_heartbeat_at"),
            registered_at: r.get("registered_at"),
            labels: r.get("labels"),
            system_info: r.get("system_info"),
            worker_token_hash: r.get("worker_token_hash"),
            registration_token_id: r.get("registration_token_id"),
            approved: r.try_get("approved").unwrap_or(true),
            description: r.get("description"),
        }
    }
}

// ============================================================================
// Storage struct and shared helpers
// ============================================================================

/// PostgreSQL storage for persistent state
pub struct Storage {
    pool: PgPool,
    schema: String,
    encryption_key: Option<String>,
}

// ── Encryption helpers ────────────────────────────────────────────────────────

/// AES-256-GCM encrypt `plaintext`. Returns `hex(nonce || ciphertext)`.
fn encrypt_value(key: &str, plaintext: &str) -> anyhow::Result<String> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    use rand::RngCore;
    use sha2::Digest;
    let key_bytes: [u8; 32] = sha2::Sha256::digest(key.as_bytes()).into();
    let cipher = Aes256Gcm::new(&key_bytes.into());
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ct);
    Ok(hex::encode(combined))
}

/// Decrypt a value produced by `encrypt_value`.
fn decrypt_value(key: &str, encoded: &str) -> anyhow::Result<String> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    use sha2::Digest;
    let combined = hex::decode(encoded).map_err(|e| anyhow::anyhow!("hex: {e}"))?;
    anyhow::ensure!(combined.len() > 12, "ciphertext too short");
    let (nonce_bytes, ct) = combined.split_at(12);
    let key_bytes: [u8; 32] = sha2::Sha256::digest(key.as_bytes()).into();
    let cipher = Aes256Gcm::new(&key_bytes.into());
    let plain = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|e| anyhow::anyhow!("decrypt: {e}"))?;
    Ok(String::from_utf8(plain)?)
}

/// Worker row from the workers table
#[derive(Debug, Clone)]
pub struct WorkerRow {
    pub worker_id: String,
    pub hostname: Option<String>,
    pub total_cpu: Option<i32>,
    pub total_memory_mb: Option<i64>,
    pub total_disk_mb: Option<i64>,
    pub disk_type: Option<String>,
    pub supported_job_types: Option<Vec<String>>,
    pub docker_enabled: bool,
    pub status: String,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub registered_at: DateTime<Utc>,
    pub labels: Option<Vec<String>>,
    pub system_info: Option<serde_json::Value>,
    pub worker_token_hash: Option<String>,
    pub registration_token_id: Option<Uuid>,
    pub approved: bool,
    pub description: Option<String>,
}

/// Job row from the jobs table (database-level job, not the in-memory Job struct)
#[derive(Debug, Clone)]
pub struct DbJob {
    pub id: Uuid,
    pub job_group_id: Uuid,
    pub stage_config_id: Option<Uuid>,
    pub stage_name: String,
    pub command: String,
    pub pre_script: Option<String>,
    pub post_script: Option<String>,
    pub worker_id: Option<String>,
    pub state: String,
    pub exit_code: Option<i32>,
    pub pre_exit_code: Option<i32>,
    pub post_exit_code: Option<i32>,
    pub log_path: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A job row joined with its group + repo info for the /runs endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunRow {
    pub id: Uuid,
    pub job_group_id: Uuid,
    pub stage_name: String,
    pub command: String,
    pub worker_id: Option<String>,
    pub state: String,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    // Joined fields
    pub branch: Option<String>,
    pub repo_name: Option<String>,
    pub group_state: String,
    pub trigger_source: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceRecommendation {
    pub recommended_cpu: i32,
    pub recommended_memory_mb: i64,
    pub recommended_disk_mb: i64,
    pub recommended_duration_secs: i32,
    pub sample_count: i64,
    pub p50_duration: f64,
    pub p90_duration: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceHistoryRow {
    pub id: Uuid,
    pub stage_config_id: Uuid,
    pub repo_id: Uuid,
    pub job_id: Uuid,
    pub actual_cpu_percent: Option<f64>,
    pub actual_memory_mb: Option<i64>,
    pub actual_disk_mb: Option<i64>,
    pub actual_duration_secs: Option<i32>,
    pub exit_code: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub id: Uuid,
    pub channel_type: String,
    pub config: serde_json::Value,
}

// ============================================================================
// Analytics structs
// ============================================================================

#[derive(Debug, serde::Serialize)]
pub struct BuildTrendPoint {
    pub date: String,
    pub total: i64,
    pub success: i64,
    pub failed: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct DurationTrendPoint {
    pub date: String,
    pub avg_duration_secs: i64,
    pub p95_duration_secs: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct SlowStage {
    pub stage_name: String,
    pub repo_name: String,
    pub avg_secs: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct FailingRepo {
    pub repo_name: String,
    pub total: i64,
    pub failed: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkerUtilization {
    pub worker_id: String,
    pub hostname: Option<String>,
    pub status: String,
    pub active_jobs: i64,
    pub total_jobs_30d: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct QueueWaitPoint {
    pub date: String,
    pub avg_wait_secs: i64,
}

/// Worker registration token (DB row)
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbWorkerToken {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub scope: String,
    pub created_by: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: i32,
    pub uses: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub worker_id: Option<String>,
}

impl From<sqlx::postgres::PgRow> for DbWorkerToken {
    fn from(r: sqlx::postgres::PgRow) -> Self {
        Self {
            id: r.get("id"),
            name: r.get("name"),
            token_hash: r.get("token_hash"),
            scope: r.try_get("scope").unwrap_or_else(|_| "shared".to_string()),
            created_by: r.get("created_by"),
            expires_at: r.get("expires_at"),
            max_uses: r.try_get("max_uses").unwrap_or(0),
            uses: r.try_get("uses").unwrap_or(0),
            active: r.try_get("active").unwrap_or(true),
            created_at: r.get("created_at"),
            worker_id: r.try_get("worker_id").ok().flatten(),
        }
    }
}

/// Label group config (DB row)
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbLabelGroup {
    pub id: Uuid,
    pub name: String,
    pub match_labels: Vec<String>,
    pub env_vars: serde_json::Value,
    pub pre_script: Option<String>,
    pub max_concurrent_jobs: i32,
    pub capabilities: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<sqlx::postgres::PgRow> for DbLabelGroup {
    fn from(r: sqlx::postgres::PgRow) -> Self {
        Self {
            id: r.get("id"),
            name: r.get("name"),
            match_labels: r.try_get("match_labels").unwrap_or_default(),
            env_vars: r
                .try_get("env_vars")
                .unwrap_or_else(|_| serde_json::json!({})),
            pre_script: r.get("pre_script"),
            max_concurrent_jobs: r.try_get("max_concurrent_jobs").unwrap_or(0),
            capabilities: r.try_get("capabilities").unwrap_or_default(),
            enabled: r.try_get("enabled").unwrap_or(true),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }
}

const WORKER_TOKEN_COLUMNS: &str =
    "id, name, token_hash, scope, created_by, expires_at, max_uses, uses, active, created_at, worker_id";

const LABEL_GROUP_COLUMNS: &str =
    "id, name, match_labels, env_vars, pre_script, max_concurrent_jobs, \
     capabilities, enabled, created_at, updated_at";

impl Storage {
    /// Create a new Storage with a connection pool.
    /// Sets `search_path` on every new connection via `after_connect`.
    pub async fn new(
        database_url: &str,
        max_connections: u32,
        schema: &str,
    ) -> anyhow::Result<Self> {
        // Validate schema name: only alphanumeric + underscore allowed
        if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
            anyhow::bail!(
                "Invalid schema name '{}': only alphanumeric and underscore allowed",
                schema
            );
        }
        let schema_owned = schema.to_string();
        let schema_for_hook = schema_owned.clone();
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .after_connect(move |conn, _meta| {
                let s = schema_for_hook.clone();
                Box::pin(async move {
                    conn.execute(format!("SET search_path TO {s}").as_str())
                        .await?;
                    Ok(())
                })
            })
            .connect(database_url)
            .await?;

        info!("Connected to PostgreSQL (search_path={})", schema_owned);
        Ok(Self {
            pool,
            schema: schema_owned,
            encryption_key: None,
        })
    }

    /// Set the AES-256-GCM encryption key for secret pipeline variables.
    pub fn with_encryption_key(mut self, key: Option<String>) -> Self {
        self.encryption_key = key;
        self
    }

    /// Run SQL migration files from the migrations/ directory.
    /// Tracks applied migrations in a `schema_migrations` table to avoid
    /// re-running already-applied SQL on every startup.
    pub async fn migrate(&self) -> anyhow::Result<()> {
        // Acquire a dedicated connection for the entire migration process
        // so SET search_path and all subsequent queries run on the same session.
        let mut conn = self.pool.acquire().await?;

        // Advisory lock prevents concurrent controller startups from racing
        sqlx::query("SELECT pg_advisory_lock(8015)")
            .execute(&mut *conn)
            .await?;

        // Ensure schema exists and set search path on this connection
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", self.schema))
            .execute(&mut *conn)
            .await?;
        sqlx::query(&format!("SET search_path TO {}", self.schema))
            .execute(&mut *conn)
            .await?;

        // Create tracking table if not exists
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INT PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                applied_at TIMESTAMPTZ DEFAULT now()
            )",
        )
        .execute(&mut *conn)
        .await?;

        let migrations: &[(i32, &str, &str)] = &[
            (
                1,
                "init_schema",
                include_str!("../../../migrations/001_init_schema.sql"),
            ),
            (
                2,
                "init_jobs",
                include_str!("../../../migrations/002_init_jobs.sql"),
            ),
            (
                3,
                "init_workers",
                include_str!("../../../migrations/003_init_workers.sql"),
            ),
            (
                4,
                "users_and_auth",
                include_str!("../../../migrations/004_users_and_auth.sql"),
            ),
            (
                5,
                "pipeline_variables",
                include_str!("../../../migrations/005_pipeline_variables.sql"),
            ),
            (
                6,
                "webhooks",
                include_str!("../../../migrations/006_webhooks.sql"),
            ),
            (
                7,
                "notifications",
                include_str!("../../../migrations/007_notifications.sql"),
            ),
            (
                8,
                "stage_depends_on",
                include_str!("../../../migrations/008_stage_depends_on.sql"),
            ),
            (
                9,
                "indexes_and_cascades",
                include_str!("../../../migrations/009_indexes_and_cascades.sql"),
            ),
            (
                10,
                "cron_schedules",
                include_str!("../../../migrations/010_cron_schedules.sql"),
            ),
            (
                11,
                "partial_indexes",
                include_str!("../../../migrations/011_partial_indexes.sql"),
            ),
            (
                12,
                "api_keys",
                include_str!("../../../migrations/012_api_keys.sql"),
            ),
            (
                13,
                "priority_labels",
                include_str!("../../../migrations/012_priority_labels.sql"),
            ),
            (
                14,
                "webhook_deliveries",
                include_str!("../../../migrations/013_webhook_deliveries.sql"),
            ),
            (
                15,
                "job_retry",
                include_str!("../../../migrations/014_job_retry.sql"),
            ),
            (
                16,
                "pr_number",
                include_str!("../../../migrations/015_pr_number.sql"),
            ),
            (
                17,
                "stage_resource_history",
                include_str!("../../../migrations/016_stage_resource_history.sql"),
            ),
            (
                18,
                "retention_cascade",
                include_str!("../../../migrations/017_retention_cascade.sql"),
            ),
            (
                19,
                "blacklists",
                include_str!("../../../migrations/018_blacklists.sql"),
            ),
            (
                20,
                "new_features",
                include_str!("../../../migrations/019_new_features.sql"),
            ),
            (
                21,
                "config_settings",
                include_str!("../../../migrations/020_config_settings.sql"),
            ),
            (
                22,
                "worker_system_info",
                include_str!("../../../migrations/023_worker_system_info.sql"),
            ),
            (
                23,
                "nullable_stage_config_id",
                include_str!("../../../migrations/021_nullable_stage_config_id.sql"),
            ),
            (
                24,
                "command_mode",
                include_str!("../../../migrations/024_command_mode.sql"),
            ),
            (
                25,
                "global_scripts",
                include_str!("../../../migrations/025_global_scripts.sql"),
            ),
            (
                26,
                "idempotency_key",
                include_str!("../../../migrations/026_idempotency_key.sql"),
            ),
            (
                27,
                "expired_state_and_resources",
                include_str!("../../../migrations/027_expired_state_and_resources.sql"),
            ),
            (
                28,
                "worker_management",
                include_str!("../../../migrations/028_worker_management.sql"),
            ),
            (
                29,
                "token_worker_binding",
                include_str!("../../../migrations/029_token_worker_binding.sql"),
            ),
        ];

        for (version, name, sql) in migrations {
            let already_applied: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = $1)",
            )
            .bind(version)
            .fetch_one(&mut *conn)
            .await?;

            if !already_applied {
                info!("Running migration {}: {}", version, name);
                // NOTE: SQL is split by ';' — migration files must not contain
                // semicolons inside string literals, dollar-quoting, or PL/pgSQL bodies.
                let mut tx = conn.begin().await?;
                for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    sqlx::query(stmt).execute(&mut *tx).await?;
                }
                sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
                    .bind(version)
                    .bind(*name)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
        }

        // Release advisory lock
        sqlx::query("SELECT pg_advisory_unlock(8015)")
            .execute(&mut *conn)
            .await?;

        info!("Migrations complete");
        Ok(())
    }

    /// Get the underlying pool reference
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ========================================================================
    // Repos
    // ========================================================================

    pub async fn get_repo_by_name(&self, repo_name: &str) -> anyhow::Result<Option<Repo>> {
        let q = format!("SELECT {REPO_COLUMNS} FROM repos WHERE repo_name = $1");
        let row = sqlx::query(&q)
            .bind(repo_name)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(map_repo))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_repo(
        &self,
        repo_name: &str,
        repo_url: &str,
        default_branch: &str,
        global_pre_script: Option<&str>,
        global_pre_script_scope: Option<&str>,
        global_post_script: Option<&str>,
        global_post_script_scope: Option<&str>,
    ) -> anyhow::Result<Repo> {
        let q = format!(
            "INSERT INTO repos (repo_name, repo_url, default_branch, \
             global_pre_script, global_pre_script_scope, \
             global_post_script, global_post_script_scope) \
             VALUES ($1, $2, $3, $4, COALESCE($5, 'worker'), $6, COALESCE($7, 'worker')) \
             RETURNING {REPO_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(repo_name)
            .bind(repo_url)
            .bind(default_branch)
            .bind(global_pre_script)
            .bind(global_pre_script_scope)
            .bind(global_post_script)
            .bind(global_post_script_scope)
            .fetch_one(&self.pool)
            .await?;

        Ok(map_repo(row))
    }

    pub async fn list_repos(&self) -> anyhow::Result<Vec<Repo>> {
        let q = format!("SELECT {REPO_COLUMNS} FROM repos ORDER BY repo_name");
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(map_repo).collect())
    }

    // ========================================================================
    // Stage Configs
    // ========================================================================

    pub async fn get_stage_configs_for_repo(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<Vec<StageConfig>> {
        let q = format!(
            "SELECT {STAGE_CONFIG_COLUMNS} FROM stage_configs \
             WHERE repo_id = $1 ORDER BY execution_order"
        );
        let rows = sqlx::query(&q).bind(repo_id).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(map_stage_config).collect())
    }

    pub async fn get_stage_config(&self, id: Uuid) -> anyhow::Result<Option<StageConfig>> {
        let q = format!("SELECT {STAGE_CONFIG_COLUMNS} FROM stage_configs WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;

        Ok(row.map(map_stage_config))
    }

    // ========================================================================
    // Stage Scripts
    // ========================================================================

    /// Get scripts for a stage, with worker_id fallback logic:
    /// First try worker-specific scripts, then fall back to generic (worker_id IS NULL).
    pub async fn get_scripts_for_stage(
        &self,
        stage_config_id: Uuid,
        worker_id: Option<&str>,
    ) -> anyhow::Result<Vec<StageScript>> {
        let rows = if let Some(wid) = worker_id {
            let q = format!(
                "SELECT {STAGE_SCRIPT_COLUMNS} FROM stage_scripts \
                 WHERE stage_config_id = $1 AND (worker_id = $2 OR worker_id IS NULL) \
                 ORDER BY \
                     CASE WHEN worker_id IS NOT NULL THEN 0 ELSE 1 END, \
                     script_type"
            );
            sqlx::query(&q)
                .bind(stage_config_id)
                .bind(wid)
                .fetch_all(&self.pool)
                .await?
        } else {
            let q = format!(
                "SELECT {STAGE_SCRIPT_COLUMNS} FROM stage_scripts \
                 WHERE stage_config_id = $1 AND worker_id IS NULL \
                 ORDER BY script_type"
            );
            sqlx::query(&q)
                .bind(stage_config_id)
                .fetch_all(&self.pool)
                .await?
        };

        // Deduplicate: prefer worker-specific over generic for each (script_type, script_scope)
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for r in rows {
            let script_type: String = r.get("script_type");
            let script_scope: String = r.get("script_scope");
            let key = (script_type, script_scope);
            if seen.insert(key) {
                result.push(map_stage_script(r));
            }
        }

        Ok(result)
    }

    // ========================================================================
    // Job Groups
    // ========================================================================

    pub async fn create_job_group(&self, group: &JobGroup) -> anyhow::Result<JobGroup> {
        let q = format!(
            "INSERT INTO job_groups (id, repo_id, branch, commit_sha, trigger_source, \
             reserved_worker_id, state, priority, pr_number, idempotency_key, \
             allocated_cpu, allocated_memory_mb, allocated_disk_mb, \
             created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
             RETURNING {JOB_GROUP_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(group.id)
            .bind(group.repo_id)
            .bind(&group.branch)
            .bind(&group.commit_sha)
            .bind(&group.trigger_source)
            .bind(&group.reserved_worker_id)
            .bind(group.state.to_string())
            .bind(group.priority)
            .bind(group.pr_number)
            .bind(&group.idempotency_key)
            .bind(group.allocated_resources.cpu as i32)
            .bind(group.allocated_resources.memory_mb as i64)
            .bind(group.allocated_resources.disk_mb as i64)
            .bind(group.created_at)
            .bind(group.updated_at)
            .fetch_one(&self.pool)
            .await?;

        Ok(map_job_group(row))
    }

    pub async fn update_job_group_state(
        &self,
        id: Uuid,
        state: JobGroupState,
    ) -> anyhow::Result<Option<JobGroup>> {
        let now = Utc::now();
        let completed_at = if state.is_terminal() { Some(now) } else { None };

        let q = format!(
            "UPDATE job_groups \
             SET state = $2, updated_at = $3, completed_at = COALESCE($4, completed_at) \
             WHERE id = $1 \
             RETURNING {JOB_GROUP_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(state.to_string())
            .bind(now)
            .bind(completed_at)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(map_job_group))
    }

    pub async fn get_job_group(&self, id: Uuid) -> anyhow::Result<Option<JobGroup>> {
        let q = format!("SELECT {JOB_GROUP_COLUMNS} FROM job_groups WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;

        Ok(row.map(map_job_group))
    }

    /// Find a non-terminal job group by idempotency key (dedup).
    pub async fn find_by_idempotency_key(&self, key: &str) -> anyhow::Result<Option<JobGroup>> {
        let q = format!(
            "SELECT {JOB_GROUP_COLUMNS} FROM job_groups \
             WHERE idempotency_key = $1 \
             AND state NOT IN ('success', 'failed', 'cancelled') \
             ORDER BY created_at DESC LIMIT 1"
        );
        let row = sqlx::query(&q).bind(key).fetch_optional(&self.pool).await?;

        Ok(row.map(map_job_group))
    }

    // ========================================================================
    // Jobs (database-level)
    // ========================================================================

    pub async fn create_job(&self, job: &DbJob) -> anyhow::Result<DbJob> {
        let q = format!(
            "INSERT INTO jobs (id, job_group_id, stage_config_id, stage_name, command, \
             pre_script, post_script, worker_id, state, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (id) DO UPDATE SET state = EXCLUDED.state, updated_at = EXCLUDED.updated_at \
             RETURNING {JOB_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(job.id)
            .bind(job.job_group_id)
            .bind(job.stage_config_id)
            .bind(&job.stage_name)
            .bind(&job.command)
            .bind(&job.pre_script)
            .bind(&job.post_script)
            .bind(&job.worker_id)
            .bind(&job.state)
            .bind(job.created_at)
            .bind(job.updated_at)
            .fetch_one(&self.pool)
            .await?;

        Ok(DbJob::from(row))
    }

    pub async fn update_job_state(
        &self,
        id: Uuid,
        state: &str,
        exit_code: Option<i32>,
        pre_exit_code: Option<i32>,
        post_exit_code: Option<i32>,
        worker_id: Option<&str>,
    ) -> anyhow::Result<Option<DbJob>> {
        let now = Utc::now();
        let started_at = if state == "running" { Some(now) } else { None };
        let completed_at = if matches!(state, "success" | "failed" | "cancelled") {
            Some(now)
        } else {
            None
        };

        let q = format!(
            "UPDATE jobs \
             SET state = $2, \
                 exit_code = COALESCE($3, exit_code), \
                 pre_exit_code = COALESCE($4, pre_exit_code), \
                 post_exit_code = COALESCE($5, post_exit_code), \
                 worker_id = COALESCE($6, worker_id), \
                 started_at = COALESCE($7, started_at), \
                 completed_at = COALESCE($8, completed_at), \
                 updated_at = $9 \
             WHERE id = $1 \
             RETURNING {JOB_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(state)
            .bind(exit_code)
            .bind(pre_exit_code)
            .bind(post_exit_code)
            .bind(worker_id)
            .bind(started_at)
            .bind(completed_at)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(DbJob::from))
    }

    pub async fn get_jobs_for_group(&self, job_group_id: Uuid) -> anyhow::Result<Vec<DbJob>> {
        let q = format!(
            "SELECT {JOB_COLUMNS} FROM jobs \
             WHERE job_group_id = $1 ORDER BY created_at"
        );
        let rows = sqlx::query(&q)
            .bind(job_group_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(DbJob::from).collect())
    }

    pub async fn get_job(&self, id: Uuid) -> anyhow::Result<Option<DbJob>> {
        let q = format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;

        Ok(row.map(DbJob::from))
    }

    // ========================================================================
    // Worker Reservations
    // ========================================================================

    pub async fn create_reservation(
        &self,
        worker_id: &str,
        job_group_id: Uuid,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<WorkerReservation> {
        let q = format!(
            "INSERT INTO worker_reservations (worker_id, job_group_id, expires_at) \
             VALUES ($1, $2, $3) \
             RETURNING {RESERVATION_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(worker_id)
            .bind(job_group_id)
            .bind(expires_at)
            .fetch_one(&self.pool)
            .await?;

        Ok(map_reservation(row))
    }

    pub async fn release_reservation(
        &self,
        worker_id: &str,
        job_group_id: Uuid,
        reason: &str,
    ) -> anyhow::Result<Option<WorkerReservation>> {
        let now = Utc::now();
        let q = format!(
            "UPDATE worker_reservations \
             SET released_at = $3, release_reason = $4 \
             WHERE worker_id = $1 AND job_group_id = $2 AND released_at IS NULL \
             RETURNING {RESERVATION_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(worker_id)
            .bind(job_group_id)
            .bind(now)
            .bind(reason)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(map_reservation))
    }

    pub async fn get_active_reservation_for_worker(
        &self,
        worker_id: &str,
    ) -> anyhow::Result<Option<WorkerReservation>> {
        let q = format!(
            "SELECT {RESERVATION_COLUMNS} FROM worker_reservations \
             WHERE worker_id = $1 AND released_at IS NULL \
             ORDER BY reserved_at DESC LIMIT 1"
        );
        let row = sqlx::query(&q)
            .bind(worker_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(map_reservation))
    }

    // ========================================================================
    // Workers
    // ========================================================================

    pub async fn upsert_worker(&self, worker: &WorkerRow) -> anyhow::Result<WorkerRow> {
        let q = format!(
            "INSERT INTO workers ({WORKER_COLUMNS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
             ON CONFLICT (worker_id) DO UPDATE \
             SET hostname = EXCLUDED.hostname, \
                 total_cpu = EXCLUDED.total_cpu, \
                 total_memory_mb = EXCLUDED.total_memory_mb, \
                 total_disk_mb = EXCLUDED.total_disk_mb, \
                 disk_type = EXCLUDED.disk_type, \
                 supported_job_types = EXCLUDED.supported_job_types, \
                 docker_enabled = EXCLUDED.docker_enabled, \
                 status = EXCLUDED.status, \
                 last_heartbeat_at = EXCLUDED.last_heartbeat_at, \
                 labels = EXCLUDED.labels, \
                 worker_token_hash = COALESCE(EXCLUDED.worker_token_hash, workers.worker_token_hash), \
                 registration_token_id = COALESCE(EXCLUDED.registration_token_id, workers.registration_token_id), \
                 description = COALESCE(EXCLUDED.description, workers.description) \
             RETURNING {WORKER_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(&worker.worker_id)
            .bind(&worker.hostname)
            .bind(worker.total_cpu)
            .bind(worker.total_memory_mb)
            .bind(worker.total_disk_mb)
            .bind(&worker.disk_type)
            .bind(&worker.supported_job_types)
            .bind(worker.docker_enabled)
            .bind(&worker.status)
            .bind(worker.last_heartbeat_at)
            .bind(worker.registered_at)
            .bind(&worker.labels)
            .bind(&worker.system_info)
            .bind(&worker.worker_token_hash)
            .bind(worker.registration_token_id)
            .bind(worker.approved)
            .bind(&worker.description)
            .fetch_one(&self.pool)
            .await?;

        Ok(WorkerRow::from(row))
    }

    pub async fn update_worker_metadata(
        &self,
        worker_id: &str,
        metadata: &serde_json::Value,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE workers SET system_info = $2 WHERE worker_id = $1")
            .bind(worker_id)
            .bind(metadata)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_worker_labels(
        &self,
        worker_id: &str,
        labels: &[String],
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE workers SET labels = $2 WHERE worker_id = $1")
            .bind(worker_id)
            .bind(labels)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_worker_status(
        &self,
        worker_id: &str,
        status: &str,
    ) -> anyhow::Result<Option<WorkerRow>> {
        let q = format!(
            "UPDATE workers SET status = $2, last_heartbeat_at = now() \
             WHERE worker_id = $1 \
             RETURNING {WORKER_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(worker_id)
            .bind(status)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(WorkerRow::from))
    }

    pub async fn get_worker(&self, worker_id: &str) -> anyhow::Result<Option<WorkerRow>> {
        let q = format!("SELECT {WORKER_COLUMNS} FROM workers WHERE worker_id = $1");
        let row = sqlx::query(&q)
            .bind(worker_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(WorkerRow::from))
    }

    /// Look up a worker by its hashed permanent token (for Flow B: reconnect auth).
    pub async fn get_worker_by_token_hash(
        &self,
        token_hash: &str,
    ) -> anyhow::Result<Option<WorkerRow>> {
        let q = format!("SELECT {WORKER_COLUMNS} FROM workers WHERE worker_token_hash = $1");
        let row = sqlx::query(&q)
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(WorkerRow::from))
    }

    pub async fn list_workers(&self) -> anyhow::Result<Vec<WorkerRow>> {
        let q = format!("SELECT {WORKER_COLUMNS} FROM workers ORDER BY worker_id");
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(WorkerRow::from).collect())
    }

    // ========================================================================
    // Users
    // ========================================================================

    pub async fn get_user_by_username(&self, username: &str) -> anyhow::Result<Option<User>> {
        let q = format!("SELECT {USER_COLUMNS} FROM users WHERE username = $1");
        let row = sqlx::query(&q)
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.as_ref().map(map_user))
    }

    pub async fn get_user(&self, id: Uuid) -> anyhow::Result<Option<User>> {
        let q = format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;

        Ok(row.as_ref().map(map_user))
    }

    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        display_name: Option<&str>,
        role: &str,
    ) -> anyhow::Result<User> {
        let q = format!(
            "INSERT INTO users (username, password_hash, display_name, role) \
             VALUES ($1, $2, $3, $4) \
             RETURNING {USER_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(username)
            .bind(password_hash)
            .bind(display_name)
            .bind(role)
            .fetch_one(&self.pool)
            .await?;

        Ok(map_user(&row))
    }

    pub async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        let q = format!("SELECT {USER_COLUMNS} FROM users ORDER BY username");
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;

        Ok(rows.iter().map(map_user).collect())
    }

    pub async fn update_user(
        &self,
        id: Uuid,
        display_name: Option<&str>,
        role: Option<&str>,
        active: Option<bool>,
        password_hash: Option<&str>,
    ) -> anyhow::Result<Option<User>> {
        let q = format!(
            "UPDATE users \
             SET display_name = COALESCE($2, display_name), \
                 role = COALESCE($3, role), \
                 active = COALESCE($4, active), \
                 password_hash = COALESCE($5, password_hash), \
                 updated_at = now() \
             WHERE id = $1 \
             RETURNING {USER_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(display_name)
            .bind(role)
            .bind(active)
            .bind(password_hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.as_ref().map(map_user))
    }

    pub async fn update_user_password(
        &self,
        id: Uuid,
        password_hash: &str,
    ) -> anyhow::Result<bool> {
        let result =
            sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(password_hash)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_user(&self, id: Uuid) -> anyhow::Result<bool> {
        let q = "DELETE FROM users WHERE id = $1";
        let result = sqlx::query(q).bind(id).execute(&self.pool).await?;

        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Sessions
    // ========================================================================

    pub async fn create_session(
        &self,
        user_id: Uuid,
        token_jti: &str,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let q = "INSERT INTO sessions (user_id, token_jti, expires_at) \
             VALUES ($1, $2, $3)";
        sqlx::query(q)
            .bind(user_id)
            .bind(token_jti)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn is_session_valid(&self, token_jti: &str) -> anyhow::Result<bool> {
        let q = "SELECT EXISTS(\
                     SELECT 1 FROM sessions \
                     WHERE token_jti = $1 AND revoked = false AND expires_at > now()\
                 )";
        let valid: bool = sqlx::query_scalar(q)
            .bind(token_jti)
            .fetch_one(&self.pool)
            .await?;

        Ok(valid)
    }

    pub async fn revoke_session(&self, token_jti: &str) -> anyhow::Result<bool> {
        let q = "UPDATE sessions SET revoked = true WHERE token_jti = $1 AND revoked = false";
        let result = sqlx::query(q).bind(token_jti).execute(&self.pool).await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_expired_sessions(&self) -> anyhow::Result<u64> {
        let q = "DELETE FROM sessions WHERE expires_at < now() OR revoked = true";
        let result = sqlx::query(q).execute(&self.pool).await?;

        Ok(result.rows_affected())
    }

    // ========================================================================
    // Audit Log
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn create_audit_log(
        &self,
        user_id: Option<Uuid>,
        username: &str,
        action: &str,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        details: Option<serde_json::Value>,
        ip_address: Option<&str>,
    ) -> anyhow::Result<()> {
        let q = "INSERT INTO audit_log \
                 (user_id, username, action, resource_type, resource_id, details, ip_address) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)";
        sqlx::query(q)
            .bind(user_id)
            .bind(username)
            .bind(action)
            .bind(resource_type)
            .bind(resource_id)
            .bind(details)
            .bind(ip_address)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn list_audit_logs(&self, limit: i64) -> anyhow::Result<Vec<serde_json::Value>> {
        let q = "SELECT id, user_id, username, action, resource_type, resource_id, \
                        details, ip_address, created_at \
                 FROM audit_log \
                 ORDER BY created_at DESC \
                 LIMIT $1";
        let rows = sqlx::query(q).bind(limit).fetch_all(&self.pool).await?;
        let entries = rows
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let user_id: Option<Uuid> = r.get("user_id");
                let username: String = r.get("username");
                let action: String = r.get("action");
                let resource_type: Option<String> = r.get("resource_type");
                let resource_id: Option<String> = r.get("resource_id");
                let details: Option<serde_json::Value> = r.get("details");
                let ip_address: Option<String> = r.get("ip_address");
                let created_at: DateTime<Utc> = r.get("created_at");
                serde_json::json!({
                    "id": id,
                    "user_id": user_id,
                    "username": username,
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "ip_address": ip_address,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect();
        Ok(entries)
    }

    // ========================================================================
    // State Recovery (startup)
    // ========================================================================

    /// Load all non-terminal job groups for state recovery on startup.
    pub async fn load_active_job_groups(&self) -> anyhow::Result<Vec<JobGroup>> {
        let q = format!(
            "SELECT {JOB_GROUP_COLUMNS} FROM job_groups \
             WHERE state NOT IN ('success', 'failed', 'cancelled') \
             ORDER BY created_at"
        );
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(map_job_group).collect())
    }

    /// Load all non-terminal jobs for state recovery on startup.
    /// Uses FOR UPDATE SKIP LOCKED to prevent lock contention when
    /// multiple controllers recover concurrently.
    pub async fn load_active_jobs(&self) -> anyhow::Result<Vec<DbJob>> {
        let q = format!(
            "SELECT {JOB_COLUMNS} FROM jobs \
             WHERE state NOT IN ('success', 'failed', 'cancelled') \
             ORDER BY created_at \
             FOR UPDATE SKIP LOCKED"
        );
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(DbJob::from).collect())
    }

    /// Load all registered workers for state recovery on startup.
    pub async fn load_workers(&self) -> anyhow::Result<Vec<WorkerRow>> {
        let q = format!("SELECT {WORKER_COLUMNS} FROM workers ORDER BY worker_id");
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(WorkerRow::from).collect())
    }

    // ========================================================================
    // Dashboard / Listing helpers
    // ========================================================================

    pub async fn list_job_groups_paginated(
        &self,
        limit: i64,
        offset: i64,
        state_filter: Option<&str>,
        repo_id_filter: Option<Uuid>,
    ) -> anyhow::Result<(Vec<JobGroup>, i64)> {
        let (where_clause, bind_offset) = match (state_filter.is_some(), repo_id_filter.is_some()) {
            (true, true) => ("WHERE state = $1 AND repo_id = $2".to_string(), 2),
            (true, false) => ("WHERE state = $1".to_string(), 1),
            (false, true) => ("WHERE repo_id = $1".to_string(), 1),
            (false, false) => (String::new(), 0),
        };

        let count_q = format!("SELECT COUNT(*) FROM job_groups {where_clause}");
        let data_q = format!(
            "SELECT {JOB_GROUP_COLUMNS} FROM job_groups {where_clause} \
             ORDER BY priority DESC, created_at DESC LIMIT ${} OFFSET ${}",
            bind_offset + 1,
            bind_offset + 2
        );

        // Build count query
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_q);
        if let Some(state) = state_filter {
            count_query = count_query.bind(state.to_string());
        }
        if let Some(repo_id) = repo_id_filter {
            count_query = count_query.bind(repo_id);
        }
        let total: i64 = count_query.fetch_one(&self.pool).await?;

        // Build data query
        let mut data_query = sqlx::query(&data_q);
        if let Some(state) = state_filter {
            data_query = data_query.bind(state.to_string());
        }
        if let Some(repo_id) = repo_id_filter {
            data_query = data_query.bind(repo_id);
        }
        data_query = data_query.bind(limit).bind(offset);

        let rows = data_query.fetch_all(&self.pool).await?;
        let groups: Vec<JobGroup> = rows.into_iter().map(map_job_group).collect();

        Ok((groups, total))
    }

    pub async fn get_job_group_with_jobs(
        &self,
        group_id: Uuid,
    ) -> anyhow::Result<Option<(JobGroup, Vec<DbJob>)>> {
        let group = self.get_job_group(group_id).await?;
        match group {
            Some(g) => {
                let jobs = self.get_jobs_for_group(group_id).await?;
                Ok(Some((g, jobs)))
            }
            None => Ok(None),
        }
    }

    // ========================================================================
    // Repos (additional CRUD)
    // ========================================================================

    pub async fn get_repo(&self, id: Uuid) -> anyhow::Result<Option<Repo>> {
        let q = format!("SELECT {REPO_COLUMNS} FROM repos WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(map_repo))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_repo(
        &self,
        id: Uuid,
        repo_name: Option<&str>,
        repo_url: Option<&str>,
        default_branch: Option<&str>,
        enabled: Option<bool>,
        max_concurrent_builds: Option<i32>,
        cancel_superseded: Option<bool>,
        global_pre_script: Option<Option<&str>>,
        global_pre_script_scope: Option<&str>,
        global_post_script: Option<Option<&str>>,
        global_post_script_scope: Option<&str>,
    ) -> anyhow::Result<Option<Repo>> {
        let q = format!(
            "UPDATE repos \
             SET repo_name = COALESCE($2, repo_name), \
                 repo_url = COALESCE($3, repo_url), \
                 default_branch = COALESCE($4, default_branch), \
                 enabled = COALESCE($5, enabled), \
                 max_concurrent_builds = COALESCE($6, max_concurrent_builds), \
                 cancel_superseded = COALESCE($7, cancel_superseded), \
                 global_pre_script = CASE WHEN $8 THEN $9 ELSE global_pre_script END, \
                 global_pre_script_scope = COALESCE($10, global_pre_script_scope), \
                 global_post_script = CASE WHEN $11 THEN $12 ELSE global_post_script END, \
                 global_post_script_scope = COALESCE($13, global_post_script_scope), \
                 updated_at = now() \
             WHERE id = $1 \
             RETURNING {REPO_COLUMNS}"
        );
        // For global scripts, we use a bool flag to distinguish "not provided" from "set to null"
        let (pre_provided, pre_val) = match global_pre_script {
            Some(v) => (true, v),
            None => (false, None),
        };
        let (post_provided, post_val) = match global_post_script {
            Some(v) => (true, v),
            None => (false, None),
        };
        let row = sqlx::query(&q)
            .bind(id)
            .bind(repo_name)
            .bind(repo_url)
            .bind(default_branch)
            .bind(enabled)
            .bind(max_concurrent_builds)
            .bind(cancel_superseded)
            .bind(pre_provided)
            .bind(pre_val)
            .bind(global_pre_script_scope)
            .bind(post_provided)
            .bind(post_val)
            .bind(global_post_script_scope)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(map_repo))
    }

    pub async fn delete_repo(&self, id: Uuid) -> anyhow::Result<bool> {
        let q = "DELETE FROM repos WHERE id = $1";
        let result = sqlx::query(q).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns (pre_script, pre_scope, post_script, post_scope) for a repo.
    pub async fn get_global_scripts(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<(Option<String>, String, Option<String>, String)> {
        let row = sqlx::query(
            "SELECT global_pre_script, \
                    COALESCE(global_pre_script_scope, 'worker') AS global_pre_script_scope, \
                    global_post_script, \
                    COALESCE(global_post_script_scope, 'worker') AS global_post_script_scope \
             FROM repos WHERE id = $1",
        )
        .bind(repo_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok((
                r.get("global_pre_script"),
                r.get("global_pre_script_scope"),
                r.get("global_post_script"),
                r.get("global_post_script_scope"),
            )),
            None => Ok((None, "worker".to_string(), None, "worker".to_string())),
        }
    }

    // ========================================================================
    // Stage Configs (additional CRUD)
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn create_stage_config(
        &self,
        repo_id: Uuid,
        stage_name: &str,
        command: Option<&str>,
        required_cpu: i32,
        required_memory_mb: i32,
        required_disk_mb: i32,
        max_duration_secs: i32,
        execution_order: i32,
        parallel_group: Option<&str>,
        allow_worker_migration: bool,
        job_type: &str,
        depends_on: Option<&[String]>,
        required_labels: Option<&[String]>,
        command_mode: &str,
    ) -> anyhow::Result<StageConfig> {
        let q = format!(
            "INSERT INTO stage_configs \
             (repo_id, stage_name, command, required_cpu, required_memory_mb, \
              required_disk_mb, max_duration_secs, execution_order, parallel_group, \
              allow_worker_migration, job_type, depends_on, required_labels, command_mode) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
             RETURNING {STAGE_CONFIG_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(repo_id)
            .bind(stage_name)
            .bind(command)
            .bind(required_cpu)
            .bind(required_memory_mb)
            .bind(required_disk_mb)
            .bind(max_duration_secs)
            .bind(execution_order)
            .bind(parallel_group)
            .bind(allow_worker_migration)
            .bind(job_type)
            .bind(depends_on)
            .bind(required_labels)
            .bind(command_mode)
            .fetch_one(&self.pool)
            .await?;
        Ok(map_stage_config(row))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_stage_config(
        &self,
        id: Uuid,
        stage_name: Option<&str>,
        command: Option<&str>,
        required_cpu: Option<i32>,
        required_memory_mb: Option<i32>,
        required_disk_mb: Option<i32>,
        max_duration_secs: Option<i32>,
        execution_order: Option<i32>,
        parallel_group: Option<&str>,
        allow_worker_migration: Option<bool>,
        job_type: Option<&str>,
        depends_on: Option<&[String]>,
        required_labels: Option<&[String]>,
        command_mode: Option<&str>,
    ) -> anyhow::Result<Option<StageConfig>> {
        let q = format!(
            "UPDATE stage_configs \
             SET stage_name = COALESCE($2, stage_name), \
                 command = COALESCE($3, command), \
                 required_cpu = COALESCE($4, required_cpu), \
                 required_memory_mb = COALESCE($5, required_memory_mb), \
                 required_disk_mb = COALESCE($6, required_disk_mb), \
                 max_duration_secs = COALESCE($7, max_duration_secs), \
                 execution_order = COALESCE($8, execution_order), \
                 parallel_group = COALESCE($9, parallel_group), \
                 allow_worker_migration = COALESCE($10, allow_worker_migration), \
                 job_type = COALESCE($11, job_type), \
                 depends_on = COALESCE($12, depends_on), \
                 required_labels = COALESCE($13, required_labels), \
                 command_mode = COALESCE($14, command_mode), \
                 updated_at = now() \
             WHERE id = $1 \
             RETURNING {STAGE_CONFIG_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(stage_name)
            .bind(command)
            .bind(required_cpu)
            .bind(required_memory_mb)
            .bind(required_disk_mb)
            .bind(max_duration_secs)
            .bind(execution_order)
            .bind(parallel_group)
            .bind(allow_worker_migration)
            .bind(job_type)
            .bind(depends_on.map(|s| s.to_vec()))
            .bind(required_labels.map(|s| s.to_vec()))
            .bind(command_mode)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(map_stage_config))
    }

    pub async fn get_stage_config_by_name(
        &self,
        repo_id: Uuid,
        stage_name: &str,
    ) -> anyhow::Result<Option<StageConfig>> {
        let q = format!(
            "SELECT {STAGE_CONFIG_COLUMNS} FROM stage_configs \
             WHERE repo_id = $1 AND stage_name = $2"
        );
        let row = sqlx::query(&q)
            .bind(repo_id)
            .bind(stage_name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(map_stage_config))
    }

    pub async fn delete_stage_config(&self, id: Uuid) -> anyhow::Result<bool> {
        let q = "DELETE FROM stage_configs WHERE id = $1";
        let result = sqlx::query(q).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Stage Scripts (CRUD)
    // ========================================================================

    pub async fn list_stage_scripts(
        &self,
        stage_config_id: Uuid,
    ) -> anyhow::Result<Vec<StageScript>> {
        let q = format!(
            "SELECT {STAGE_SCRIPT_COLUMNS} FROM stage_scripts \
             WHERE stage_config_id = $1 ORDER BY script_type, script_scope"
        );
        let rows = sqlx::query(&q)
            .bind(stage_config_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(map_stage_script).collect())
    }

    pub async fn get_stage_script(&self, id: Uuid) -> anyhow::Result<Option<StageScript>> {
        let q = format!("SELECT {STAGE_SCRIPT_COLUMNS} FROM stage_scripts WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(map_stage_script))
    }

    pub async fn create_stage_script(
        &self,
        stage_config_id: Uuid,
        script_type: &str,
        script_scope: &str,
        script: &str,
        worker_id: Option<&str>,
    ) -> anyhow::Result<StageScript> {
        let q = format!(
            "INSERT INTO stage_scripts \
             (id, stage_config_id, worker_id, script_type, script_scope, script, \
              created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, now(), now()) \
             RETURNING {STAGE_SCRIPT_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(stage_config_id)
            .bind(worker_id)
            .bind(script_type)
            .bind(script_scope)
            .bind(script)
            .fetch_one(&self.pool)
            .await?;
        Ok(map_stage_script(row))
    }

    pub async fn update_stage_script(
        &self,
        id: Uuid,
        script_type: Option<&str>,
        script_scope: Option<&str>,
        script: Option<&str>,
        worker_id: Option<Option<&str>>,
    ) -> anyhow::Result<Option<StageScript>> {
        let q = format!(
            "UPDATE stage_scripts SET \
             script_type  = COALESCE($2, script_type), \
             script_scope = COALESCE($3, script_scope), \
             script       = COALESCE($4, script), \
             worker_id    = CASE WHEN $5 THEN $6 ELSE worker_id END, \
             updated_at   = now() \
             WHERE id = $1 \
             RETURNING {STAGE_SCRIPT_COLUMNS}"
        );
        let (update_worker, new_worker): (bool, Option<&str>) = match worker_id {
            Some(w) => (true, w),
            None => (false, None),
        };
        let row = sqlx::query(&q)
            .bind(id)
            .bind(script_type)
            .bind(script_scope)
            .bind(script)
            .bind(update_worker)
            .bind(new_worker)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(map_stage_script))
    }

    pub async fn delete_stage_script(&self, id: Uuid) -> anyhow::Result<bool> {
        let q = "DELETE FROM stage_scripts WHERE id = $1";
        let result = sqlx::query(q).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_users_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<(Vec<User>, i64)> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        let q = format!(
            "SELECT {} FROM users ORDER BY username LIMIT {} OFFSET {}",
            USER_COLUMNS, limit, offset
        );
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok((rows.into_iter().map(|r| map_user(&r)).collect(), total))
    }

    pub async fn list_repos_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<(Vec<Repo>, i64)> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repos")
            .fetch_one(&self.pool)
            .await?;
        let q = format!(
            "SELECT {} FROM repos ORDER BY repo_name LIMIT {} OFFSET {}",
            REPO_COLUMNS, limit, offset
        );
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok((rows.into_iter().map(map_repo).collect(), total))
    }

    pub async fn get_jobs_for_group_paginated(
        &self,
        group_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<(Vec<DbJob>, i64)> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE job_group_id = $1")
            .bind(group_id)
            .fetch_one(&self.pool)
            .await?;
        let q = format!(
            "SELECT {} FROM jobs WHERE job_group_id = $1 ORDER BY created_at LIMIT {} OFFSET {}",
            JOB_COLUMNS, limit, offset
        );
        let rows = sqlx::query(&q).bind(group_id).fetch_all(&self.pool).await?;
        Ok((rows.into_iter().map(DbJob::from).collect(), total))
    }

    /// List individual job runs with group + repo context.
    pub async fn list_runs_paginated(
        &self,
        limit: i64,
        offset: i64,
        state_filter: Option<&str>,
        worker_filter: Option<&str>,
    ) -> anyhow::Result<(Vec<RunRow>, i64)> {
        let mut conditions: Vec<String> = Vec::new();
        let mut bind_idx = 0u32;
        if state_filter.is_some() {
            bind_idx += 1;
            conditions.push(format!("j.state = ${bind_idx}"));
        }
        if worker_filter.is_some() {
            bind_idx += 1;
            conditions.push(format!("j.worker_id = ${bind_idx}"));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_q = format!("SELECT COUNT(*) FROM jobs j {where_clause}");
        let data_q = format!(
            "SELECT j.id, j.job_group_id, j.stage_name, j.command, j.worker_id, \
             j.state, j.exit_code, j.started_at, j.completed_at, j.created_at, \
             jg.branch, jg.state AS group_state, jg.trigger_source, \
             r.repo_name \
             FROM jobs j \
             JOIN job_groups jg ON j.job_group_id = jg.id \
             LEFT JOIN repos r ON jg.repo_id = r.id \
             {where_clause} \
             ORDER BY j.created_at DESC LIMIT ${} OFFSET ${}",
            bind_idx + 1,
            bind_idx + 2
        );

        let mut count_query = sqlx::query_scalar::<_, i64>(&count_q);
        if let Some(s) = state_filter {
            count_query = count_query.bind(s.to_string());
        }
        if let Some(w) = worker_filter {
            count_query = count_query.bind(w.to_string());
        }
        let total: i64 = count_query.fetch_one(&self.pool).await?;

        let mut data_query = sqlx::query(&data_q);
        if let Some(s) = state_filter {
            data_query = data_query.bind(s.to_string());
        }
        if let Some(w) = worker_filter {
            data_query = data_query.bind(w.to_string());
        }
        data_query = data_query.bind(limit).bind(offset);

        let rows = data_query.fetch_all(&self.pool).await?;
        let runs = rows
            .into_iter()
            .map(|r| RunRow {
                id: r.get("id"),
                job_group_id: r.get("job_group_id"),
                stage_name: r.get("stage_name"),
                command: r.get("command"),
                worker_id: r.get("worker_id"),
                state: r.get("state"),
                exit_code: r.get("exit_code"),
                started_at: r.get("started_at"),
                completed_at: r.get("completed_at"),
                created_at: r.get("created_at"),
                branch: r.get("branch"),
                repo_name: r.get("repo_name"),
                group_state: r.get("group_state"),
                trigger_source: r.get("trigger_source"),
            })
            .collect();

        Ok((runs, total))
    }

    pub async fn get_notification_configs_for_trigger(
        &self,
        repo_id: Uuid,
        event_type: &str,
    ) -> anyhow::Result<Vec<NotificationConfig>> {
        let rows = sqlx::query(
            "SELECT id, channel_type, config FROM notification_configs WHERE repo_id = $1 AND trigger = $2"
        )
        .bind(repo_id)
        .bind(event_type)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| NotificationConfig {
                id: r.get("id"),
                channel_type: r.get("channel_type"),
                config: r.get("config"),
            })
            .collect())
    }

    // ── Notification CRUD ────────────────────────────────────────────────────

    pub async fn list_notification_configs(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, repo_id, trigger, channel_type, config, enabled, created_at \
             FROM notification_configs WHERE repo_id = $1 ORDER BY created_at",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let rid: Uuid = r.get("repo_id");
                let trigger: String = r.get("trigger");
                let channel_type: String = r.get("channel_type");
                let config: serde_json::Value = r.get("config");
                let enabled: bool = r.get("enabled");
                let created_at: DateTime<Utc> = r.get("created_at");
                serde_json::json!({
                    "id": id, "repo_id": rid, "trigger": trigger,
                    "channel_type": channel_type, "config": config,
                    "enabled": enabled, "created_at": created_at.to_rfc3339(),
                })
            })
            .collect())
    }

    pub async fn create_notification_config(
        &self,
        repo_id: Uuid,
        trigger: &str,
        channel_type: &str,
        config: serde_json::Value,
        enabled: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let row = sqlx::query(
            "INSERT INTO notification_configs (repo_id, trigger, channel_type, config, enabled) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, repo_id, trigger, channel_type, config, enabled, created_at",
        )
        .bind(repo_id)
        .bind(trigger)
        .bind(channel_type)
        .bind(&config)
        .bind(enabled)
        .fetch_one(&self.pool)
        .await?;
        let id: Uuid = row.get("id");
        let rid: Uuid = row.get("repo_id");
        let trig: String = row.get("trigger");
        let ct: String = row.get("channel_type");
        let cfg: serde_json::Value = row.get("config");
        let en: bool = row.get("enabled");
        let ca: DateTime<Utc> = row.get("created_at");
        Ok(serde_json::json!({
            "id": id, "repo_id": rid, "trigger": trig,
            "channel_type": ct, "config": cfg,
            "enabled": en, "created_at": ca.to_rfc3339(),
        }))
    }

    pub async fn update_notification_config(
        &self,
        id: Uuid,
        enabled: bool,
        config: serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let row = sqlx::query(
            "UPDATE notification_configs SET enabled = $1, config = $2 WHERE id = $3 \
             RETURNING id, repo_id, trigger, channel_type, config, enabled, created_at",
        )
        .bind(enabled)
        .bind(&config)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            let rid: Uuid = r.get("repo_id");
            let trig: String = r.get("trigger");
            let ct: String = r.get("channel_type");
            let cfg: serde_json::Value = r.get("config");
            let en: bool = r.get("enabled");
            let ca: DateTime<Utc> = r.get("created_at");
            serde_json::json!({
                "id": id, "repo_id": rid, "trigger": trig,
                "channel_type": ct, "config": cfg,
                "enabled": en, "created_at": ca.to_rfc3339(),
            })
        }))
    }

    pub async fn delete_notification_config(&self, id: Uuid) -> anyhow::Result<bool> {
        let res = sqlx::query("DELETE FROM notification_configs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    // ── Cron Schedule CRUD ───────────────────────────────────────────────────

    pub async fn list_cron_schedules_for_repo(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<Vec<CronSchedule>> {
        let rows = sqlx::query(
            "SELECT id, repo_id, interval_secs, next_run_at, stages, branch, enabled, \
                    last_triggered_at, created_at, updated_at \
             FROM cron_schedules WHERE repo_id = $1 ORDER BY created_at",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| CronSchedule {
                id: r.get("id"),
                repo_id: r.get("repo_id"),
                interval_secs: r.get("interval_secs"),
                next_run_at: r.get("next_run_at"),
                stages: r.get("stages"),
                branch: r.get("branch"),
                enabled: r.get("enabled"),
                last_triggered_at: r.get("last_triggered_at"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn create_cron_schedule(
        &self,
        repo_id: Uuid,
        interval_secs: i64,
        stages: &[String],
        branch: &str,
    ) -> anyhow::Result<CronSchedule> {
        let row = sqlx::query(
            "INSERT INTO cron_schedules (repo_id, interval_secs, stages, branch) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, repo_id, interval_secs, next_run_at, stages, branch, enabled, \
                       last_triggered_at, created_at, updated_at",
        )
        .bind(repo_id)
        .bind(interval_secs)
        .bind(stages)
        .bind(branch)
        .fetch_one(&self.pool)
        .await?;
        Ok(CronSchedule {
            id: row.get("id"),
            repo_id: row.get("repo_id"),
            interval_secs: row.get("interval_secs"),
            next_run_at: row.get("next_run_at"),
            stages: row.get("stages"),
            branch: row.get("branch"),
            enabled: row.get("enabled"),
            last_triggered_at: row.get("last_triggered_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn update_cron_schedule(
        &self,
        id: Uuid,
        interval_secs: Option<i64>,
        stages: Option<&[String]>,
        branch: Option<&str>,
        enabled: Option<bool>,
    ) -> anyhow::Result<Option<CronSchedule>> {
        let row = sqlx::query(
            "UPDATE cron_schedules SET \
                interval_secs = COALESCE($1, interval_secs), \
                stages = COALESCE($2, stages), \
                branch = COALESCE($3, branch), \
                enabled = COALESCE($4, enabled), \
                updated_at = now() \
             WHERE id = $5 \
             RETURNING id, repo_id, interval_secs, next_run_at, stages, branch, enabled, \
                       last_triggered_at, created_at, updated_at",
        )
        .bind(interval_secs)
        .bind(stages)
        .bind(branch)
        .bind(enabled)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| CronSchedule {
            id: r.get("id"),
            repo_id: r.get("repo_id"),
            interval_secs: r.get("interval_secs"),
            next_run_at: r.get("next_run_at"),
            stages: r.get("stages"),
            branch: r.get("branch"),
            enabled: r.get("enabled"),
            last_triggered_at: r.get("last_triggered_at"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn delete_cron_schedule(&self, id: Uuid) -> anyhow::Result<bool> {
        let res = sqlx::query("DELETE FROM cron_schedules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn get_stage_dependencies(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
        let rows = sqlx::query_as::<_, (String, Vec<String>)>(
            "SELECT stage_name, depends_on FROM stage_configs WHERE repo_id = $1 ORDER BY execution_order"
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    pub async fn list_due_schedules(&self) -> anyhow::Result<Vec<CronSchedule>> {
        let q = "SELECT id, repo_id, interval_secs, next_run_at, stages, branch, enabled, \
             last_triggered_at, created_at, updated_at \
             FROM cron_schedules WHERE enabled = true AND next_run_at <= now()";
        let rows = sqlx::query(q).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CronSchedule {
                id: r.get("id"),
                repo_id: r.get("repo_id"),
                interval_secs: r.get("interval_secs"),
                next_run_at: r.get("next_run_at"),
                stages: r.get("stages"),
                branch: r.get("branch"),
                enabled: r.get("enabled"),
                last_triggered_at: r.get("last_triggered_at"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    // ── Pipeline variables ────────────────────────────────────────────────

    fn maybe_encrypt(&self, value: &str, is_secret: bool) -> anyhow::Result<String> {
        match (is_secret, &self.encryption_key) {
            (true, Some(key)) => encrypt_value(key, value),
            _ => Ok(value.to_string()),
        }
    }

    fn maybe_decrypt(&self, raw: String, is_secret: bool) -> String {
        match (is_secret, &self.encryption_key) {
            (true, Some(key)) => decrypt_value(key, &raw).unwrap_or(raw),
            _ => raw,
        }
    }

    pub async fn list_variables_for_repo(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<Vec<PipelineVariable>> {
        let rows = sqlx::query(
            "SELECT id, repo_id, name, value, is_secret, created_at, updated_at
             FROM pipeline_variables WHERE repo_id = $1 ORDER BY name",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let is_secret: bool = r.get("is_secret");
                PipelineVariable {
                    id: r.get("id"),
                    repo_id: r.get("repo_id"),
                    name: r.get("name"),
                    value: self.maybe_decrypt(r.get("value"), is_secret),
                    is_secret,
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                }
            })
            .collect())
    }

    pub async fn create_variable(
        &self,
        repo_id: Uuid,
        name: &str,
        value: &str,
        is_secret: bool,
    ) -> anyhow::Result<PipelineVariable> {
        let stored = self.maybe_encrypt(value, is_secret)?;
        let row = sqlx::query(
            "INSERT INTO pipeline_variables (id, repo_id, name, value, is_secret)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, repo_id, name, value, is_secret, created_at, updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(repo_id)
        .bind(name)
        .bind(stored)
        .bind(is_secret)
        .fetch_one(&self.pool)
        .await?;

        Ok(PipelineVariable {
            id: row.get("id"),
            repo_id: row.get("repo_id"),
            name: row.get("name"),
            value: value.to_string(), // return plaintext to caller
            is_secret: row.get("is_secret"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn update_variable(
        &self,
        id: Uuid,
        name: Option<&str>,
        value: Option<&str>,
        is_secret: Option<bool>,
    ) -> anyhow::Result<Option<PipelineVariable>> {
        // If value is being updated, determine whether to encrypt it
        let encrypted_value: Option<String> = if let Some(v) = value {
            let will_secret = match is_secret {
                Some(b) => b,
                None => sqlx::query("SELECT is_secret FROM pipeline_variables WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|r| r.get::<bool, _>("is_secret"))
                    .unwrap_or(false),
            };
            Some(self.maybe_encrypt(v, will_secret)?)
        } else {
            None
        };

        let row = sqlx::query(
            "UPDATE pipeline_variables
             SET name = COALESCE($2, name),
                 value = COALESCE($3, value),
                 is_secret = COALESCE($4, is_secret),
                 updated_at = now()
             WHERE id = $1
             RETURNING id, repo_id, name, value, is_secret, created_at, updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(encrypted_value.as_deref())
        .bind(is_secret)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let is_secret: bool = r.get("is_secret");
            PipelineVariable {
                id: r.get("id"),
                repo_id: r.get("repo_id"),
                name: r.get("name"),
                value: self.maybe_decrypt(r.get("value"), is_secret),
                is_secret,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }
        }))
    }

    pub async fn delete_variable(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM pipeline_variables WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Get secret variable values for a repo (log masking). Returns plaintext.
    pub async fn get_secret_values_for_repo(&self, repo_id: Uuid) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT value FROM pipeline_variables
             WHERE repo_id = $1 AND is_secret = true",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| self.maybe_decrypt(r.get::<String, _>("value"), true))
            .collect())
    }

    pub async fn mark_schedule_triggered(&self, schedule_id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE cron_schedules SET last_triggered_at = now() WHERE id = $1")
            .bind(schedule_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Webhooks ─────────────────────────────────────────────────────────────

    pub async fn get_webhook_by_secret(
        &self,
        secret: &str,
    ) -> anyhow::Result<Option<ci_core::models::stage::Webhook>> {
        let row = sqlx::query(
            "SELECT id, repo_id, provider, secret, events, enabled, created_at, updated_at \
             FROM webhooks WHERE secret = $1",
        )
        .bind(secret)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| ci_core::models::stage::Webhook {
            id: r.get("id"),
            repo_id: r.get("repo_id"),
            provider: r.get("provider"),
            secret: r.get("secret"),
            events: r.get("events"),
            enabled: r.get("enabled"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn list_webhooks_for_repo(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<Vec<ci_core::models::stage::Webhook>> {
        let rows = sqlx::query(
            "SELECT id, repo_id, provider, secret, events, enabled, created_at, updated_at \
             FROM webhooks WHERE repo_id = $1 ORDER BY created_at",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ci_core::models::stage::Webhook {
                id: r.get("id"),
                repo_id: r.get("repo_id"),
                provider: r.get("provider"),
                secret: r.get("secret"),
                events: r.get("events"),
                enabled: r.get("enabled"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn create_webhook(
        &self,
        repo_id: Uuid,
        provider: &str,
        secret: &str,
        events: &[String],
    ) -> anyhow::Result<ci_core::models::stage::Webhook> {
        let row = sqlx::query(
            "INSERT INTO webhooks (repo_id, provider, secret, events) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, repo_id, provider, secret, events, enabled, created_at, updated_at",
        )
        .bind(repo_id)
        .bind(provider)
        .bind(secret)
        .bind(events)
        .fetch_one(&self.pool)
        .await?;
        Ok(ci_core::models::stage::Webhook {
            id: row.get("id"),
            repo_id: row.get("repo_id"),
            provider: row.get("provider"),
            secret: row.get("secret"),
            events: row.get("events"),
            enabled: row.get("enabled"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn delete_webhook(&self, webhook_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
            .bind(webhook_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // API Keys
    // ========================================================================

    pub async fn create_api_key(
        &self,
        user_id: Uuid,
        key_hash: &str,
        name: &str,
    ) -> anyhow::Result<ApiKey> {
        let q = format!(
            "INSERT INTO api_keys (user_id, key_hash, name) \
             VALUES ($1, $2, $3) \
             RETURNING {API_KEY_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(user_id)
            .bind(key_hash)
            .bind(name)
            .fetch_one(&self.pool)
            .await?;
        Ok(map_api_key(&row))
    }

    /// Look up an active (non-revoked) API key by its SHA-256 hash.
    /// Also bumps last_used_at.
    pub async fn get_api_key_by_hash(&self, key_hash: &str) -> anyhow::Result<Option<ApiKey>> {
        let row = sqlx::query(
            "UPDATE api_keys SET last_used_at = now() \
             WHERE key_hash = $1 AND revoked = false \
             RETURNING id, user_id, name, created_at, last_used_at, revoked",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().map(map_api_key))
    }

    pub async fn list_api_keys_for_user(&self, user_id: Uuid) -> anyhow::Result<Vec<ApiKey>> {
        let q = format!(
            "SELECT {API_KEY_COLUMNS} FROM api_keys \
             WHERE user_id = $1 ORDER BY created_at DESC"
        );
        let rows = sqlx::query(&q).bind(user_id).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(map_api_key).collect())
    }

    pub async fn revoke_api_key(&self, id: Uuid, user_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE api_keys SET revoked = true \
             WHERE id = $1 AND user_id = $2 AND revoked = false",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Badge — latest job group state for a repo
    // ========================================================================

    pub async fn get_latest_job_group_for_repo(
        &self,
        repo_id: Uuid,
    ) -> anyhow::Result<Option<JobGroup>> {
        let q = format!(
            "SELECT {JOB_GROUP_COLUMNS} FROM job_groups \
             WHERE repo_id = $1 ORDER BY created_at DESC LIMIT 1"
        );
        let row = sqlx::query(&q)
            .bind(repo_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(map_job_group))
    }

    // ========================================================================
    // Webhook deliveries
    // ========================================================================

    pub async fn record_webhook_delivery(
        &self,
        webhook_id: Uuid,
        event: &str,
        status_code: Option<i32>,
        response_time_ms: Option<i32>,
        error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO webhook_deliveries              (webhook_id, event, status_code, response_time_ms, error_message)              VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(webhook_id)
        .bind(event)
        .bind(status_code)
        .bind(response_time_ms)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_webhook_deliveries(
        &self,
        webhook_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, webhook_id, event, status_code, response_time_ms,              error_message, created_at              FROM webhook_deliveries WHERE webhook_id = $1              ORDER BY created_at DESC LIMIT $2",
        )
        .bind(webhook_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let event: String = r.get("event");
                let status_code: Option<i32> = r.get("status_code");
                let response_time_ms: Option<i32> = r.get("response_time_ms");
                let error_message: Option<String> = r.get("error_message");
                let created_at: DateTime<Utc> = r.get("created_at");
                serde_json::json!({
                    "id": id,
                    "webhook_id": webhook_id,
                    "event": event,
                    "status_code": status_code,
                    "response_time_ms": response_time_ms,
                    "error_message": error_message,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect())
    }

    // ========================================================================
    // Job retry
    // ========================================================================

    /// Re-queue a failed job: set state=queued, increment retry_count.
    pub async fn retry_job(&self, id: Uuid) -> anyhow::Result<Option<DbJob>> {
        let q = format!(
            "UPDATE jobs              SET state = 'queued', retry_count = retry_count + 1,                  exit_code = NULL, started_at = NULL, completed_at = NULL,                  updated_at = now()              WHERE id = $1 AND state IN ('failed', 'cancelled')              RETURNING {JOB_COLUMNS}"
        );
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(DbJob::from))
    }

    /// Get max_retries for a job's stage_config.
    pub async fn get_max_retries_for_job(&self, job_id: Uuid) -> anyhow::Result<i32> {
        let result: Option<i32> = sqlx::query_scalar(
            "SELECT sc.max_retries FROM jobs j              JOIN stage_configs sc ON sc.id = j.stage_config_id              WHERE j.id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(result.unwrap_or(0))
    }

    // ========================================================================
    // Stage Resource History
    // ========================================================================

    /// Record actual resource usage after a stage completes.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_stage_resources(
        &self,
        stage_config_id: Uuid,
        repo_id: Uuid,
        job_id: Uuid,
        cpu: Option<f64>,
        memory_mb: Option<i64>,
        disk_mb: Option<i64>,
        duration_secs: Option<i32>,
        exit_code: Option<i32>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO stage_resource_history \
             (stage_config_id, repo_id, job_id, actual_cpu_percent, actual_memory_mb, \
              actual_disk_mb, actual_duration_secs, exit_code) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(stage_config_id)
        .bind(repo_id)
        .bind(job_id)
        .bind(cpu)
        .bind(memory_mb)
        .bind(disk_mb)
        .bind(duration_secs)
        .bind(exit_code)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get p90 resource recommendations from the last 20 successful runs.
    pub async fn get_resource_recommendations(
        &self,
        stage_config_id: Uuid,
    ) -> anyhow::Result<Option<ResourceRecommendation>> {
        let row = sqlx::query(
            "WITH recent AS ( \
                SELECT actual_cpu_percent, actual_memory_mb, actual_disk_mb, actual_duration_secs \
                FROM stage_resource_history \
                WHERE stage_config_id = $1 AND exit_code = 0 \
                ORDER BY created_at DESC LIMIT 20 \
            ) \
            SELECT \
                COUNT(*) AS sample_count, \
                PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY actual_cpu_percent) AS p90_cpu, \
                PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY actual_memory_mb) AS p90_memory, \
                PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY actual_disk_mb) AS p90_disk, \
                PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY actual_duration_secs) AS p50_duration, \
                PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY actual_duration_secs) AS p90_duration \
            FROM recent",
        )
        .bind(stage_config_id)
        .fetch_one(&self.pool)
        .await?;

        let sample_count: i64 = row.get("sample_count");
        if sample_count == 0 {
            return Ok(None);
        }

        let p90_cpu: Option<f64> = row.get("p90_cpu");
        let p90_memory: Option<f64> = row.get("p90_memory");
        let p90_disk: Option<f64> = row.get("p90_disk");
        let p50_duration: Option<f64> = row.get("p50_duration");
        let p90_duration: Option<f64> = row.get("p90_duration");

        Ok(Some(ResourceRecommendation {
            recommended_cpu: p90_cpu.unwrap_or(0.0).ceil() as i32,
            recommended_memory_mb: p90_memory.unwrap_or(0.0).ceil() as i64,
            recommended_disk_mb: p90_disk.unwrap_or(0.0).ceil() as i64,
            recommended_duration_secs: p90_duration.unwrap_or(0.0).ceil() as i32,
            sample_count,
            p50_duration: p50_duration.unwrap_or(0.0),
            p90_duration: p90_duration.unwrap_or(0.0),
        }))
    }

    // ========================================================================
    // Retention / Cleanup
    // ========================================================================

    pub async fn find_expired_groups(&self, max_age_days: i32) -> anyhow::Result<Vec<Uuid>> {
        let q = format!(
            "SELECT id FROM {s}.job_groups \
             WHERE completed_at < NOW() - make_interval(days => $1) \
             AND state IN ('success', 'failed', 'cancelled') \
             ORDER BY completed_at ASC LIMIT 1000",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(max_age_days)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    pub async fn find_excess_groups_per_repo(
        &self,
        max_per_repo: i32,
    ) -> anyhow::Result<Vec<Uuid>> {
        let q = format!(
            "WITH ranked AS ( \
                SELECT id, repo_id, ROW_NUMBER() OVER (PARTITION BY repo_id ORDER BY created_at DESC) as rn \
                FROM {s}.job_groups \
                WHERE state IN ('success', 'failed', 'cancelled') \
            ) SELECT id FROM ranked WHERE rn > $1 LIMIT 5000",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(max_per_repo)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    pub async fn delete_job_groups_batch(&self, ids: &[Uuid]) -> anyhow::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let q = format!(
            "DELETE FROM {s}.job_groups WHERE id = ANY($1)",
            s = self.schema
        );
        let result = sqlx::query(&q).bind(ids).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// List recent resource history entries for a stage.
    pub async fn list_resource_history(
        &self,
        stage_config_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<ResourceHistoryRow>> {
        let rows = sqlx::query(
            "SELECT id, stage_config_id, repo_id, job_id, actual_cpu_percent, \
             actual_memory_mb, actual_disk_mb, actual_duration_secs, exit_code, created_at \
             FROM stage_resource_history \
             WHERE stage_config_id = $1 \
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(stage_config_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ResourceHistoryRow {
                id: r.get("id"),
                stage_config_id: r.get("stage_config_id"),
                repo_id: r.get("repo_id"),
                job_id: r.get("job_id"),
                actual_cpu_percent: r.get("actual_cpu_percent"),
                actual_memory_mb: r.get("actual_memory_mb"),
                actual_disk_mb: r.get("actual_disk_mb"),
                actual_duration_secs: r.get("actual_duration_secs"),
                exit_code: r.get("exit_code"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    // ========================================================================
    // Analytics
    // ========================================================================

    pub async fn get_build_trends(&self, days: i32) -> anyhow::Result<Vec<BuildTrendPoint>> {
        let q = format!(
            "SELECT DATE(created_at)::text as date, COUNT(*)::bigint as total, \
             COUNT(*) FILTER (WHERE state = 'success')::bigint as success, \
             COUNT(*) FILTER (WHERE state = 'failed')::bigint as failed \
             FROM {s}.job_groups WHERE created_at > NOW() - make_interval(days => $1) \
             GROUP BY DATE(created_at) ORDER BY date",
            s = self.schema
        );
        let rows = sqlx::query(&q).bind(days).fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| BuildTrendPoint {
                date: r.get("date"),
                total: r.get("total"),
                success: r.get("success"),
                failed: r.get("failed"),
            })
            .collect())
    }

    pub async fn get_duration_trends(&self, days: i32) -> anyhow::Result<Vec<DurationTrendPoint>> {
        let q = format!(
            "SELECT DATE(created_at)::text as date, \
             COALESCE(AVG(EXTRACT(EPOCH FROM (completed_at - created_at)))::bigint, 0) as avg_duration_secs, \
             COALESCE(PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM (completed_at - created_at)))::bigint, 0) as p95_duration_secs \
             FROM {s}.job_groups WHERE completed_at IS NOT NULL AND created_at > NOW() - make_interval(days => $1) \
             GROUP BY DATE(created_at) ORDER BY date",
            s = self.schema
        );
        let rows = sqlx::query(&q).bind(days).fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| DurationTrendPoint {
                date: r.get("date"),
                avg_duration_secs: r.get("avg_duration_secs"),
                p95_duration_secs: r.get("p95_duration_secs"),
            })
            .collect())
    }

    pub async fn get_slowest_stages(
        &self,
        days: i32,
        limit: i32,
    ) -> anyhow::Result<Vec<SlowStage>> {
        let q = format!(
            "SELECT sc.stage_name, r.repo_name, \
             COALESCE(AVG(EXTRACT(EPOCH FROM (j.completed_at - j.started_at)))::bigint, 0) as avg_secs \
             FROM {s}.jobs j JOIN {s}.stage_configs sc ON j.stage_config_id = sc.id \
             JOIN {s}.repos r ON sc.repo_id = r.id \
             WHERE j.completed_at IS NOT NULL AND j.started_at IS NOT NULL \
             AND j.created_at > NOW() - make_interval(days => $1) \
             GROUP BY sc.stage_name, r.repo_name ORDER BY avg_secs DESC LIMIT $2",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(days)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| SlowStage {
                stage_name: r.get("stage_name"),
                repo_name: r.get("repo_name"),
                avg_secs: r.get("avg_secs"),
            })
            .collect())
    }

    pub async fn get_most_failing_repos(
        &self,
        days: i32,
        limit: i32,
    ) -> anyhow::Result<Vec<FailingRepo>> {
        let q = format!(
            "SELECT r.repo_name, COUNT(*)::bigint as total, \
             COUNT(*) FILTER (WHERE jg.state = 'failed')::bigint as failed \
             FROM {s}.job_groups jg JOIN {s}.repos r ON jg.repo_id = r.id \
             WHERE jg.created_at > NOW() - make_interval(days => $1) \
             GROUP BY r.repo_name ORDER BY failed DESC LIMIT $2",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(days)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| FailingRepo {
                repo_name: r.get("repo_name"),
                total: r.get("total"),
                failed: r.get("failed"),
            })
            .collect())
    }

    pub async fn get_worker_utilization(&self) -> anyhow::Result<Vec<WorkerUtilization>> {
        let q = format!(
            "SELECT w.worker_id, w.hostname, w.status::text, \
             COUNT(j.id) FILTER (WHERE j.state = 'running')::bigint as active_jobs, \
             COUNT(j.id)::bigint as total_jobs_30d \
             FROM {s}.workers w LEFT JOIN {s}.jobs j ON j.worker_id = w.worker_id \
             AND j.created_at > NOW() - INTERVAL '30 days' \
             GROUP BY w.worker_id, w.hostname, w.status",
            s = self.schema
        );
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| WorkerUtilization {
                worker_id: r.get("worker_id"),
                hostname: r.get("hostname"),
                status: r.get("status"),
                active_jobs: r.get("active_jobs"),
                total_jobs_30d: r.get("total_jobs_30d"),
            })
            .collect())
    }

    pub async fn get_queue_wait_trends(&self, days: i32) -> anyhow::Result<Vec<QueueWaitPoint>> {
        let q = format!(
            "SELECT DATE(created_at)::text as date, \
             COALESCE(AVG(EXTRACT(EPOCH FROM (started_at - created_at)))::bigint, 0) as avg_wait_secs \
             FROM {s}.jobs WHERE started_at IS NOT NULL AND created_at > NOW() - make_interval(days => $1) \
             GROUP BY DATE(created_at) ORDER BY date",
            s = self.schema
        );
        let rows = sqlx::query(&q).bind(days).fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| QueueWaitPoint {
                date: r.get("date"),
                avg_wait_secs: r.get("avg_wait_secs"),
            })
            .collect())
    }

    // ========================================================================
    // Command Blacklist
    // ========================================================================

    pub async fn list_command_blacklist(
        &self,
        repo_id: Option<Uuid>,
        stage_config_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let mut q = String::from(
            "SELECT id, repo_id, stage_config_id, pattern, description, enabled, created_at \
             FROM stage_command_blacklist WHERE 1=1",
        );
        let mut binds: Vec<Option<Uuid>> = Vec::new();
        if let Some(rid) = repo_id {
            binds.push(Some(rid));
            q.push_str(&format!(" AND repo_id = ${}", binds.len()));
        }
        if let Some(sid) = stage_config_id {
            binds.push(Some(sid));
            q.push_str(&format!(" AND stage_config_id = ${}", binds.len()));
        }
        q.push_str(" ORDER BY created_at DESC");

        let mut query = sqlx::query(&q);
        for b in &binds {
            query = query.bind(*b);
        }
        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let repo_id: Option<Uuid> = r.get("repo_id");
                let stage_config_id: Option<Uuid> = r.get("stage_config_id");
                let pattern: String = r.get("pattern");
                let description: Option<String> = r.get("description");
                let enabled: bool = r.get("enabled");
                let created_at: DateTime<Utc> = r.get("created_at");
                serde_json::json!({
                    "id": id,
                    "repo_id": repo_id,
                    "stage_config_id": stage_config_id,
                    "pattern": pattern,
                    "description": description,
                    "enabled": enabled,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect())
    }

    pub async fn create_command_blacklist(
        &self,
        repo_id: Option<Uuid>,
        stage_config_id: Option<Uuid>,
        pattern: &str,
        description: Option<&str>,
    ) -> anyhow::Result<Uuid> {
        let row = sqlx::query(
            "INSERT INTO stage_command_blacklist (repo_id, stage_config_id, pattern, description) \
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(repo_id)
        .bind(stage_config_id)
        .bind(pattern)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn update_command_blacklist(
        &self,
        id: Uuid,
        pattern: Option<&str>,
        description: Option<&str>,
        enabled: Option<bool>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE stage_command_blacklist \
             SET pattern = COALESCE($2, pattern), \
                 description = COALESCE($3, description), \
                 enabled = COALESCE($4, enabled), \
                 updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(pattern)
        .bind(description)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_command_blacklist(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM stage_command_blacklist WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_active_command_blacklist(
        &self,
        repo_id: Uuid,
        stage_config_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT pattern FROM stage_command_blacklist \
             WHERE enabled = true \
               AND (repo_id IS NULL OR repo_id = $1) \
               AND (stage_config_id IS NULL OR stage_config_id = $2)",
        )
        .bind(repo_id)
        .bind(stage_config_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("pattern")).collect())
    }

    // ========================================================================
    // Branch Blacklist
    // ========================================================================

    pub async fn list_branch_blacklist(
        &self,
        worker_id: Option<&str>,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let (q, filter) = match worker_id {
            Some(wid) => (
                "SELECT id, worker_id, pattern, description, enabled, created_at \
                 FROM worker_branch_blacklist WHERE worker_id = $1 \
                 ORDER BY created_at DESC"
                    .to_string(),
                Some(wid.to_string()),
            ),
            None => (
                "SELECT id, worker_id, pattern, description, enabled, created_at \
                 FROM worker_branch_blacklist \
                 ORDER BY created_at DESC"
                    .to_string(),
                None,
            ),
        };

        let mut query = sqlx::query(&q);
        if let Some(ref wid) = filter {
            query = query.bind(wid);
        }
        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let worker_id: String = r.get("worker_id");
                let pattern: String = r.get("pattern");
                let description: Option<String> = r.get("description");
                let enabled: bool = r.get("enabled");
                let created_at: DateTime<Utc> = r.get("created_at");
                serde_json::json!({
                    "id": id,
                    "worker_id": worker_id,
                    "pattern": pattern,
                    "description": description,
                    "enabled": enabled,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect())
    }

    pub async fn create_branch_blacklist(
        &self,
        worker_id: &str,
        pattern: &str,
        description: Option<&str>,
    ) -> anyhow::Result<Uuid> {
        let row = sqlx::query(
            "INSERT INTO worker_branch_blacklist (worker_id, pattern, description) \
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(worker_id)
        .bind(pattern)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn update_branch_blacklist(
        &self,
        id: Uuid,
        pattern: Option<&str>,
        description: Option<&str>,
        enabled: Option<bool>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE worker_branch_blacklist \
             SET pattern = COALESCE($2, pattern), \
                 description = COALESCE($3, description), \
                 enabled = COALESCE($4, enabled), \
                 updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(pattern)
        .bind(description)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_branch_blacklist(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM worker_branch_blacklist WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_active_branch_blacklist(
        &self,
        worker_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT pattern FROM worker_branch_blacklist \
             WHERE worker_id = $1 AND enabled = true",
        )
        .bind(worker_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("pattern")).collect())
    }

    // ========================================================================
    // Artifacts
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_artifact(
        &self,
        group_id: Uuid,
        job_id: Option<Uuid>,
        stage_name: &str,
        filename: &str,
        file_path: &str,
        size_bytes: i64,
        content_type: &str,
    ) -> anyhow::Result<Uuid> {
        let row = sqlx::query(
            "INSERT INTO artifacts \
             (job_group_id, job_id, stage_name, filename, file_path, size_bytes, content_type) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(group_id)
        .bind(job_id)
        .bind(stage_name)
        .bind(filename)
        .bind(file_path)
        .bind(size_bytes)
        .bind(content_type)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn list_artifacts_for_group(
        &self,
        group_id: Uuid,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, job_group_id, job_id, stage_name, filename, file_path, \
                    size_bytes, content_type, created_at \
             FROM artifacts WHERE job_group_id = $1 ORDER BY created_at",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let job_id: Option<Uuid> = r.get("job_id");
                let stage_name: String = r.get("stage_name");
                let filename: String = r.get("filename");
                let size_bytes: i64 = r.get("size_bytes");
                let content_type: String = r.get("content_type");
                let created_at: DateTime<Utc> = r.get("created_at");
                serde_json::json!({
                    "id": id,
                    "job_id": job_id,
                    "stage_name": stage_name,
                    "filename": filename,
                    "size_bytes": size_bytes,
                    "content_type": content_type,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect())
    }

    /// Returns (file_path, filename, content_type)
    pub async fn get_artifact(
        &self,
        artifact_id: Uuid,
    ) -> anyhow::Result<Option<(String, String, String)>> {
        let row =
            sqlx::query("SELECT file_path, filename, content_type FROM artifacts WHERE id = $1")
                .bind(artifact_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| {
            (
                r.get::<String, _>("file_path"),
                r.get::<String, _>("filename"),
                r.get::<String, _>("content_type"),
            )
        }))
    }

    pub async fn delete_artifacts_for_group(&self, group_id: Uuid) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM artifacts WHERE job_group_id = $1")
            .bind(group_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // ========================================================================
    // Concurrency controls
    // ========================================================================

    /// Count active (non-terminal) job groups for a repo.
    pub async fn count_active_groups_for_repo(&self, repo_id: Uuid) -> anyhow::Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM job_groups \
             WHERE repo_id = $1 AND state NOT IN ('completed', 'failed', 'cancelled')",
        )
        .bind(repo_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Find active groups on the same repo+branch that would be superseded.
    /// Excludes `exclude_id` (the new group being created).
    pub async fn find_superseded_groups(
        &self,
        repo_id: Uuid,
        branch: &str,
        exclude_id: Uuid,
    ) -> anyhow::Result<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT id FROM job_groups \
             WHERE repo_id = $1 AND branch = $2 AND id != $3 \
               AND state NOT IN ('completed', 'failed', 'cancelled') \
             ORDER BY created_at ASC",
        )
        .bind(repo_id)
        .bind(branch)
        .bind(exclude_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    // ── Config Settings ───────────────────────────────────────────────────

    pub async fn get_all_config_settings(
        &self,
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let rows = sqlx::query("SELECT key, value FROM config_settings")
            .fetch_all(&self.pool)
            .await?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            map.insert(r.get::<String, _>("key"), r.get::<String, _>("value"));
        }
        Ok(map)
    }

    pub async fn get_config_setting(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM config_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    pub async fn set_config_setting(
        &self,
        key: &str,
        value: &str,
        description: Option<&str>,
        updated_by: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO config_settings (key, value, description, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, NOW()) \
             ON CONFLICT (key) DO UPDATE SET \
               value = $2, \
               description = COALESCE($3, config_settings.description), \
               updated_by = $4, \
               updated_at = NOW()",
        )
        .bind(key)
        .bind(value)
        .bind(description)
        .bind(updated_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_config_setting(&self, key: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM config_settings WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Worker Tokens
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn create_worker_token(
        &self,
        name: &str,
        token_hash: &str,
        scope: &str,
        created_by: Option<&str>,
        expires_at: Option<DateTime<Utc>>,
        max_uses: i32,
        worker_id: Option<&str>,
    ) -> anyhow::Result<DbWorkerToken> {
        let q = format!(
            "INSERT INTO worker_tokens (name, token_hash, scope, created_by, expires_at, max_uses, worker_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING {WORKER_TOKEN_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(name)
            .bind(token_hash)
            .bind(scope)
            .bind(created_by)
            .bind(expires_at)
            .bind(max_uses)
            .bind(worker_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(DbWorkerToken::from(row))
    }

    pub async fn get_worker_token_by_hash(
        &self,
        hash: &str,
    ) -> anyhow::Result<Option<DbWorkerToken>> {
        let q = format!("SELECT {WORKER_TOKEN_COLUMNS} FROM worker_tokens WHERE token_hash = $1");
        let row = sqlx::query(&q)
            .bind(hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(DbWorkerToken::from))
    }

    /// Return the bound worker_id for a token (if any).
    pub async fn get_token_worker_id(&self, hash: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT worker_id FROM worker_tokens WHERE token_hash = $1")
                .bind(hash)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(wid,)| wid))
    }

    pub async fn list_worker_tokens(&self) -> anyhow::Result<Vec<DbWorkerToken>> {
        let q =
            format!("SELECT {WORKER_TOKEN_COLUMNS} FROM worker_tokens ORDER BY created_at DESC");
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(DbWorkerToken::from).collect())
    }

    pub async fn increment_worker_token_uses(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE worker_tokens SET uses = uses + 1 WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_worker_token_active(&self, id: Uuid, active: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE worker_tokens SET active = $2 WHERE id = $1")
            .bind(id)
            .bind(active)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_worker_token(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM worker_tokens WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Deactivate all active tokens bound to a specific worker_id.
    /// Returns the number of rows updated.
    pub async fn deactivate_tokens_for_worker(&self, worker_id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "UPDATE worker_tokens SET active = false WHERE worker_id = $1 AND active = true",
        )
        .bind(worker_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Validate a registration token: must be active, not expired, and under
    /// the max_uses limit (max_uses=0 means unlimited).
    pub async fn validate_registration_token(&self, hash: &str) -> anyhow::Result<DbWorkerToken> {
        let token = self
            .get_worker_token_by_hash(hash)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Registration token not found"))?;

        if !token.active {
            anyhow::bail!("Registration token is inactive");
        }
        if let Some(exp) = token.expires_at {
            if exp < Utc::now() {
                anyhow::bail!("Registration token has expired");
            }
        }
        if token.max_uses > 0 && token.uses >= token.max_uses {
            anyhow::bail!("Registration token has reached max uses");
        }
        Ok(token)
    }

    /// Register a worker: create worker row + generate token, return token plaintext.
    /// This is the admin flow -- pre-registers a worker and generates its token.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_worker(
        &self,
        worker_id: &str,
        hostname: &str,
        labels: &[String],
        description: Option<&str>,
        token_name: &str,
        token_hash: &str,
        created_by: &str,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        // 1. Upsert worker row
        sqlx::query(
            "INSERT INTO workers (worker_id, hostname, status, registered_at, docker_enabled, labels, description, approved) \
             VALUES ($1, $2, 'offline', now(), false, $3, $4, true) \
             ON CONFLICT (worker_id) DO UPDATE \
             SET hostname = EXCLUDED.hostname, \
                 labels = EXCLUDED.labels, \
                 description = COALESCE(EXCLUDED.description, workers.description)"
        )
        .bind(worker_id)
        .bind(hostname)
        .bind(labels)
        .bind(description)
        .execute(&mut *tx)
        .await?;

        // 2. Create worker_token row with worker_id binding
        sqlx::query(
            "INSERT INTO worker_tokens (name, token_hash, scope, created_by, worker_id) \
             VALUES ($1, $2, 'dedicated', $3, $4)",
        )
        .bind(token_name)
        .bind(token_hash)
        .bind(created_by)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    // ========================================================================
    // Label Groups
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub async fn create_label_group(
        &self,
        name: &str,
        match_labels: &[String],
        env_vars: Option<&serde_json::Value>,
        pre_script: Option<&str>,
        max_concurrent_jobs: Option<i32>,
        capabilities: &[String],
        enabled: bool,
    ) -> anyhow::Result<DbLabelGroup> {
        let default_env = serde_json::json!({});
        let ev = env_vars.unwrap_or(&default_env);
        let mcj = max_concurrent_jobs.unwrap_or(0);
        let q = format!(
            "INSERT INTO label_groups (name, match_labels, env_vars, pre_script, \
             max_concurrent_jobs, capabilities, enabled) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING {LABEL_GROUP_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(name)
            .bind(match_labels)
            .bind(ev)
            .bind(pre_script)
            .bind(mcj)
            .bind(capabilities)
            .bind(enabled)
            .fetch_one(&self.pool)
            .await?;
        Ok(DbLabelGroup::from(row))
    }

    pub async fn list_label_groups(&self) -> anyhow::Result<Vec<DbLabelGroup>> {
        let q = format!("SELECT {LABEL_GROUP_COLUMNS} FROM label_groups ORDER BY name");
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(DbLabelGroup::from).collect())
    }

    pub async fn get_label_group(&self, id: Uuid) -> anyhow::Result<Option<DbLabelGroup>> {
        let q = format!("SELECT {LABEL_GROUP_COLUMNS} FROM label_groups WHERE id = $1");
        let row = sqlx::query(&q).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(DbLabelGroup::from))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_label_group(
        &self,
        id: Uuid,
        name: Option<&str>,
        match_labels: Option<&[String]>,
        env_vars: Option<&serde_json::Value>,
        pre_script: Option<&str>,
        max_concurrent_jobs: Option<i32>,
        capabilities: Option<&[String]>,
        enabled: Option<bool>,
    ) -> anyhow::Result<Option<DbLabelGroup>> {
        let q = format!(
            "UPDATE label_groups SET \
             name = COALESCE($2, name), \
             match_labels = COALESCE($3, match_labels), \
             env_vars = COALESCE($4, env_vars), \
             pre_script = COALESCE($5, pre_script), \
             max_concurrent_jobs = COALESCE($6, max_concurrent_jobs), \
             capabilities = COALESCE($7, capabilities), \
             enabled = COALESCE($8, enabled), \
             updated_at = now() \
             WHERE id = $1 \
             RETURNING {LABEL_GROUP_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(name)
            .bind(match_labels)
            .bind(env_vars)
            .bind(pre_script)
            .bind(max_concurrent_jobs)
            .bind(capabilities)
            .bind(enabled)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(DbLabelGroup::from))
    }

    pub async fn delete_label_group(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM label_groups WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns label groups where ALL match_labels are a subset of the given
    /// worker_labels. Only returns enabled groups.
    pub async fn get_matching_label_groups(
        &self,
        worker_labels: &[String],
    ) -> anyhow::Result<Vec<DbLabelGroup>> {
        let q = format!(
            "SELECT {LABEL_GROUP_COLUMNS} FROM label_groups \
             WHERE enabled = true AND match_labels <@ $1 \
             ORDER BY name"
        );
        let rows = sqlx::query(&q)
            .bind(worker_labels)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(DbLabelGroup::from).collect())
    }

    // ========================================================================
    // Enhanced worker persistence
    // ========================================================================

    pub async fn update_worker_approved(
        &self,
        worker_id: &str,
        approved: bool,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE workers SET approved = $2 WHERE worker_id = $1")
            .bind(worker_id)
            .bind(approved)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_worker(&self, worker_id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM workers WHERE worker_id = $1")
            .bind(worker_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
