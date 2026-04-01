use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Acquire, Executor, PgPool, Row};
use tracing::info;
use uuid::Uuid;

use ci_core::models::job_group::{JobGroup, JobGroupState};
use ci_core::models::schedule::CronSchedule;
use ci_core::models::stage::{Repo, StageConfig, StageScript, WorkerReservation};
use ci_core::models::user::{User, UserRole};
use ci_core::models::variable::PipelineVariable;

// ============================================================================
// Column list constants (prevent drift between SELECT / INSERT / RETURNING)
// ============================================================================

const REPO_COLUMNS: &str =
    "id, repo_name, repo_url, default_branch, enabled, created_at, updated_at";

const STAGE_CONFIG_COLUMNS: &str =
    "id, repo_id, stage_name, command, required_cpu, required_memory_mb, \
     required_disk_mb, max_duration_secs, execution_order, parallel_group, \
     allow_worker_migration, job_type, created_at, updated_at";

const STAGE_SCRIPT_COLUMNS: &str =
    "id, stage_config_id, worker_id, script_type, script_scope, script, \
     created_at, updated_at";

const JOB_GROUP_COLUMNS: &str =
    "id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id, \
     state, created_at, updated_at, completed_at";

const JOB_COLUMNS: &str = "id, job_group_id, stage_config_id, stage_name, command, pre_script, \
     post_script, worker_id, state, exit_code, pre_exit_code, post_exit_code, \
     log_path, started_at, completed_at, created_at, updated_at";

const WORKER_COLUMNS: &str =
    "worker_id, hostname, total_cpu, total_memory_mb, total_disk_mb, disk_type, \
     supported_job_types, docker_enabled, status, last_heartbeat_at, registered_at";

const RESERVATION_COLUMNS: &str =
    "id, worker_id, job_group_id, reserved_at, expires_at, released_at, release_reason";

const USER_COLUMNS: &str =
    "id, username, password_hash, display_name, role, active, created_at, updated_at";

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
    JobGroup {
        id: r.get("id"),
        repo_id: r.get("repo_id"),
        branch: r.get("branch"),
        commit_sha: r.get("commit_sha"),
        trigger_source: r.get("trigger_source"),
        reserved_worker_id: r.get("reserved_worker_id"),
        state: JobGroupState::from_str(&state_str),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        completed_at: r.get("completed_at"),
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
}

/// Job row from the jobs table (database-level job, not the in-memory Job struct)
#[derive(Debug, Clone)]
pub struct DbJob {
    pub id: Uuid,
    pub job_group_id: Uuid,
    pub stage_config_id: Uuid,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub id: Uuid,
    pub channel_type: String,
    pub config: serde_json::Value,
}

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
        })
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

    pub async fn create_repo(
        &self,
        repo_name: &str,
        repo_url: &str,
        default_branch: &str,
    ) -> anyhow::Result<Repo> {
        let q = format!(
            "INSERT INTO repos (repo_name, repo_url, default_branch) \
             VALUES ($1, $2, $3) \
             RETURNING {REPO_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(repo_name)
            .bind(repo_url)
            .bind(default_branch)
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
             reserved_worker_id, state, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
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

    /// Find an active (pending/reserved/running) job group for a repo+branch+commit combo.
    pub async fn find_active_job_group(
        &self,
        repo_id: Uuid,
        branch: Option<&str>,
        commit_sha: Option<&str>,
    ) -> anyhow::Result<Option<JobGroup>> {
        let q = format!(
            "SELECT {JOB_GROUP_COLUMNS} FROM job_groups \
             WHERE repo_id = $1 \
             AND branch IS NOT DISTINCT FROM $2 \
             AND commit_sha IS NOT DISTINCT FROM $3 \
             AND state IN ('pending', 'reserved', 'running') \
             ORDER BY created_at DESC LIMIT 1"
        );
        let row = sqlx::query(&q)
            .bind(repo_id)
            .bind(branch)
            .bind(commit_sha)
            .fetch_optional(&self.pool)
            .await?;

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
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (worker_id) DO UPDATE \
             SET hostname = EXCLUDED.hostname, \
                 total_cpu = EXCLUDED.total_cpu, \
                 total_memory_mb = EXCLUDED.total_memory_mb, \
                 total_disk_mb = EXCLUDED.total_disk_mb, \
                 disk_type = EXCLUDED.disk_type, \
                 supported_job_types = EXCLUDED.supported_job_types, \
                 docker_enabled = EXCLUDED.docker_enabled, \
                 status = EXCLUDED.status, \
                 last_heartbeat_at = EXCLUDED.last_heartbeat_at \
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
            .fetch_one(&self.pool)
            .await?;

        Ok(WorkerRow::from(row))
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
        let q = format!(
            "INSERT INTO sessions (user_id, token_jti, expires_at) \
             VALUES ($1, $2, $3)"
        );
        sqlx::query(&q)
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
    pub async fn load_active_jobs(&self) -> anyhow::Result<Vec<DbJob>> {
        let q = format!(
            "SELECT {JOB_COLUMNS} FROM jobs \
             WHERE state NOT IN ('success', 'failed', 'cancelled') \
             ORDER BY created_at"
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
             ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
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

    pub async fn update_repo(
        &self,
        id: Uuid,
        repo_name: Option<&str>,
        repo_url: Option<&str>,
        default_branch: Option<&str>,
        enabled: Option<bool>,
    ) -> anyhow::Result<Option<Repo>> {
        let q = format!(
            "UPDATE repos \
             SET repo_name = COALESCE($2, repo_name), \
                 repo_url = COALESCE($3, repo_url), \
                 default_branch = COALESCE($4, default_branch), \
                 enabled = COALESCE($5, enabled), \
                 updated_at = now() \
             WHERE id = $1 \
             RETURNING {REPO_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(repo_name)
            .bind(repo_url)
            .bind(default_branch)
            .bind(enabled)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(map_repo))
    }

    pub async fn delete_repo(&self, id: Uuid) -> anyhow::Result<bool> {
        let q = "DELETE FROM repos WHERE id = $1";
        let result = sqlx::query(q).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Stage Configs (additional CRUD)
    // ========================================================================

    pub async fn create_stage_config(
        &self,
        repo_id: Uuid,
        stage_name: &str,
        command: &str,
        required_cpu: i32,
        required_memory_mb: i32,
        required_disk_mb: i32,
        max_duration_secs: i32,
        execution_order: i32,
        parallel_group: Option<&str>,
        allow_worker_migration: bool,
        job_type: &str,
        depends_on: Option<&[String]>,
    ) -> anyhow::Result<StageConfig> {
        let q = format!(
            "INSERT INTO stage_configs \
             (repo_id, stage_name, command, required_cpu, required_memory_mb, \
              required_disk_mb, max_duration_secs, execution_order, parallel_group, \
              allow_worker_migration, job_type) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
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
            .fetch_one(&self.pool)
            .await?;
        Ok(map_stage_config(row))
    }

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
                 updated_at = now() \
             WHERE id = $1 \
             RETURNING {STAGE_CONFIG_COLUMNS}"
        );
        let new_depends = depends_on;
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
            .bind(new_depends.map(|s| s.to_vec()))
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
        let q = format!(
            "SELECT id, repo_id, interval_secs, next_run_at, stages, branch, enabled,              last_triggered_at, created_at, updated_at              FROM cron_schedules WHERE enabled = true AND next_run_at <= now()"
        );
        let rows = sqlx::query(&q).fetch_all(&self.pool).await?;
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
            .map(|r| PipelineVariable {
                id: r.get("id"),
                repo_id: r.get("repo_id"),
                name: r.get("name"),
                value: r.get("value"),
                is_secret: r.get("is_secret"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
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
        let row = sqlx::query(
            "INSERT INTO pipeline_variables (id, repo_id, name, value, is_secret)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, repo_id, name, value, is_secret, created_at, updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(repo_id)
        .bind(name)
        .bind(value)
        .bind(is_secret)
        .fetch_one(&self.pool)
        .await?;

        Ok(PipelineVariable {
            id: row.get("id"),
            repo_id: row.get("repo_id"),
            name: row.get("name"),
            value: row.get("value"),
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
        .bind(value)
        .bind(is_secret)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| PipelineVariable {
            id: r.get("id"),
            repo_id: r.get("repo_id"),
            name: r.get("name"),
            value: r.get("value"),
            is_secret: r.get("is_secret"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn delete_variable(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM pipeline_variables WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Get secret variable values for a repo (for log masking).
    pub async fn get_secret_values_for_repo(&self, repo_id: Uuid) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT value FROM pipeline_variables
             WHERE repo_id = $1 AND is_secret = true",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| r.get::<String, _>("value")).collect())
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
}
