use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use tracing::info;
use uuid::Uuid;

use ci_core::models::job_group::{JobGroup, JobGroupState};
use ci_core::models::stage::{Repo, StageConfig, StageScript, WorkerReservation};

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
}

/// Worker row from the workers table
#[derive(Debug, Clone)]
pub struct WorkerRow {
    pub worker_id: String,
    pub hostname: Option<String>,
    pub total_cpu: Option<i32>,
    pub total_memory_mb: Option<i32>,
    pub total_disk_mb: Option<i32>,
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

impl Storage {
    /// Create a new Storage with a connection pool
    pub async fn new(database_url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;

        info!("Connected to PostgreSQL");
        Ok(Self { pool })
    }

    /// Run SQL migration files from the migrations/ directory
    pub async fn migrate(&self) -> anyhow::Result<()> {
        let migration_files = [
            include_str!("../../../migrations/001_create_repos.sql"),
            include_str!("../../../migrations/002_create_stage_configs.sql"),
            include_str!("../../../migrations/003_create_stage_scripts.sql"),
            include_str!("../../../migrations/004_create_job_groups.sql"),
            include_str!("../../../migrations/005_create_jobs.sql"),
            include_str!("../../../migrations/006_create_worker_reservations.sql"),
            include_str!("../../../migrations/007_create_workers.sql"),
        ];

        for sql in &migration_files {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        info!("Database migrations complete");
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

    // ========================================================================
    // Jobs (database-level)
    // ========================================================================

    pub async fn create_job(&self, job: &DbJob) -> anyhow::Result<DbJob> {
        let q = format!(
            "INSERT INTO jobs (id, job_group_id, stage_config_id, stage_name, command, \
             pre_script, post_script, worker_id, state, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
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
}
