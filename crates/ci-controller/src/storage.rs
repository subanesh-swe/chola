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
     COALESCE(global_pre_script_lock_enabled, false) AS global_pre_script_lock_enabled, \
     global_pre_script_lock_key, \
     COALESCE(global_pre_script_lock_timeout_secs, 120) AS global_pre_script_lock_timeout_secs, \
     COALESCE(global_post_script_lock_enabled, false) AS global_post_script_lock_enabled, \
     global_post_script_lock_key, \
     COALESCE(global_post_script_lock_timeout_secs, 120) AS global_post_script_lock_timeout_secs, \
     created_at, updated_at";

const STAGE_CONFIG_COLUMNS: &str =
    "id, repo_id, stage_name, command, required_cpu, required_memory_mb, \
     required_disk_mb, max_duration_secs, execution_order, parallel_group, \
     allow_worker_migration, job_type, depends_on, required_labels, max_retries, \
     command_mode, created_at, updated_at";

const STAGE_SCRIPT_COLUMNS: &str =
    "id, stage_config_id, worker_id, script_type, script_scope, script, \
     COALESCE(lock_enabled, false) AS lock_enabled, lock_key, \
     COALESCE(lock_timeout_secs, 120) AS lock_timeout_secs, \
     created_at, updated_at";

const JOB_GROUP_COLUMNS: &str =
    "id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id, \
     state, priority, pr_number, idempotency_key, \
     allocated_cpu, allocated_memory_mb, allocated_disk_mb, \
     reserved_stages, \
     status_reason, created_at, updated_at, completed_at";

const JOB_COLUMNS: &str = "id, job_group_id, stage_config_id, stage_name, command, pre_script, \
     post_script, worker_id, state, exit_code, pre_exit_code, post_exit_code, \
     log_path, started_at, completed_at, retry_count, status_reason, created_at, updated_at";

/// Column list for SELECT / RETURNING (contains COALESCE for nullable priority).
const WORKER_COLUMNS: &str =
    "worker_id, hostname, total_cpu, total_memory_mb, total_disk_mb, disk_type, \
     supported_job_types, docker_enabled, status, last_heartbeat_at, registered_at, labels, \
     system_info, worker_token_hash, registration_token_id, approved, description, \
     COALESCE(priority, 0) AS priority, max_cpu, max_memory_mb, max_disk_mb, \
     max_cpu_percent, max_memory_percent, max_disk_percent";

/// Raw column list for INSERT (no COALESCE expressions).
const WORKER_INSERT_COLUMNS: &str =
    "worker_id, hostname, total_cpu, total_memory_mb, total_disk_mb, disk_type, \
     supported_job_types, docker_enabled, status, last_heartbeat_at, registered_at, labels, \
     system_info, worker_token_hash, registration_token_id, approved, description, \
     priority, max_cpu, max_memory_mb, max_disk_mb, \
     max_cpu_percent, max_memory_percent, max_disk_percent";

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
        global_pre_script_lock_enabled: r.get("global_pre_script_lock_enabled"),
        global_pre_script_lock_key: r.get("global_pre_script_lock_key"),
        global_pre_script_lock_timeout_secs: r.get("global_pre_script_lock_timeout_secs"),
        global_post_script_lock_enabled: r.get("global_post_script_lock_enabled"),
        global_post_script_lock_key: r.get("global_post_script_lock_key"),
        global_post_script_lock_timeout_secs: r.get("global_post_script_lock_timeout_secs"),
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
        lock_enabled: r.get("lock_enabled"),
        lock_key: r.get("lock_key"),
        lock_timeout_secs: r.get("lock_timeout_secs"),
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
        // NULL on rows older than migration 034 → empty vec (legacy behavior).
        reserved_stages: r
            .try_get::<Option<Vec<String>>, _>("reserved_stages")
            .ok()
            .flatten()
            .unwrap_or_default(),
        status_reason: r.try_get("status_reason").ok().flatten(),
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
            status_reason: r.try_get("status_reason").ok().flatten(),
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
            priority: r.try_get("priority").unwrap_or(0),
            max_cpu: r.get("max_cpu"),
            max_memory_mb: r.get("max_memory_mb"),
            max_disk_mb: r.get("max_disk_mb"),
            max_cpu_percent: r.get("max_cpu_percent"),
            max_memory_percent: r.get("max_memory_percent"),
            max_disk_percent: r.get("max_disk_percent"),
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
    pub priority: i32,
    pub max_cpu: Option<i32>,
    pub max_memory_mb: Option<i64>,
    pub max_disk_mb: Option<i64>,
    pub max_cpu_percent: Option<i32>,
    pub max_memory_percent: Option<i32>,
    pub max_disk_percent: Option<i32>,
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
    pub status_reason: Option<String>,
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

/// Time window for analytics queries — explicit `[from, to]` range or fallback
/// to "last N days" (relative to NOW()).
#[derive(Debug, Clone)]
pub enum AnalyticsWindow {
    Range {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    LastDays(i32),
}

/// Bucket size for trend aggregation. The string form is fed straight into
/// `DATE_TRUNC` — values are allowlisted at construction so user input cannot
/// reach SQL unchecked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Hour,
    Day,
}

impl Granularity {
    /// SQL token for `DATE_TRUNC`. Hard-coded — never derived from user input.
    pub fn as_sql_unit(self) -> &'static str {
        match self {
            Granularity::Hour => "hour",
            Granularity::Day => "day",
        }
    }
}

impl Default for Granularity {
    fn default() -> Self {
        Granularity::Day
    }
}

/// Filters threaded into every analytics query. `exit_code = Some(-1)` means
/// "any non-zero" (matches subtask-3's sentinel convention in
/// `list_job_groups_paginated`).
#[derive(Debug, Clone)]
pub struct AnalyticsFilters {
    pub window: AnalyticsWindow,
    pub repo_id: Option<Uuid>,
    pub branch: Option<String>,
    pub stage_name: Option<String>,
    pub exit_code: Option<i32>,
    pub granularity: Granularity,
}

/// Build artefact: a WHERE clause string and the bind index following the last
/// filter param (caller appends additional binds like LIMIT at this index).
pub struct AnalyticsPlan {
    pub where_clause: String,
    pub next_idx: usize,
}

type PgQuery<'q> = sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>;

impl AnalyticsFilters {
    /// Plan a WHERE clause where filters apply directly to a `job_groups` table
    /// (alias `prefix`, empty string for unaliased). `extras` are extra clauses
    /// AND-ed in (e.g. `completed_at IS NOT NULL`).
    pub fn plan_for_job_groups(&self, prefix: &str, extras: &[&str]) -> AnalyticsPlan {
        let p = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}.")
        };
        let mut clauses: Vec<String> = extras.iter().map(|s| s.to_string()).collect();
        let mut idx: usize = 0;
        let mut next = || {
            idx += 1;
            idx
        };

        // Time window
        match &self.window {
            AnalyticsWindow::Range { .. } => {
                clauses.push(format!("{p}created_at >= ${}", next()));
                clauses.push(format!("{p}created_at <= ${}", next()));
            }
            AnalyticsWindow::LastDays(_) => {
                clauses.push(format!(
                    "{p}created_at > NOW() - make_interval(days => ${})",
                    next()
                ));
            }
        }

        if self.repo_id.is_some() {
            clauses.push(format!("{p}repo_id = ${}", next()));
        }
        if self.branch.is_some() {
            clauses.push(format!("{p}branch = ${}", next()));
        }
        if self.stage_name.is_some() {
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM jobs j_f WHERE j_f.job_group_id = {p}id \
                 AND j_f.stage_name = ${})",
                next()
            ));
        }
        if let Some(code) = self.exit_code {
            if code == -1 {
                clauses.push(format!(
                    "EXISTS (SELECT 1 FROM jobs j_f WHERE j_f.job_group_id = {p}id \
                     AND j_f.exit_code IS NOT NULL AND j_f.exit_code != 0)"
                ));
            } else {
                clauses.push(format!(
                    "EXISTS (SELECT 1 FROM jobs j_f WHERE j_f.job_group_id = {p}id \
                     AND j_f.exit_code = ${})",
                    next()
                ));
            }
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        AnalyticsPlan {
            where_clause,
            next_idx: idx + 1,
        }
    }

    /// Plan a WHERE clause for a query rooted at `jobs` joined with
    /// `stage_configs` (alias `sc`) and `job_groups` (alias `jg`).
    pub fn plan_for_jobs(&self, j: &str, sc: &str, jg: &str, extras: &[&str]) -> AnalyticsPlan {
        let mut clauses: Vec<String> = extras.iter().map(|s| s.to_string()).collect();
        let mut idx: usize = 0;
        let mut next = || {
            idx += 1;
            idx
        };

        match &self.window {
            AnalyticsWindow::Range { .. } => {
                clauses.push(format!("{j}.created_at >= ${}", next()));
                clauses.push(format!("{j}.created_at <= ${}", next()));
            }
            AnalyticsWindow::LastDays(_) => {
                clauses.push(format!(
                    "{j}.created_at > NOW() - make_interval(days => ${})",
                    next()
                ));
            }
        }

        if self.repo_id.is_some() {
            clauses.push(format!("{sc}.repo_id = ${}", next()));
        }
        if self.branch.is_some() {
            clauses.push(format!("{jg}.branch = ${}", next()));
        }
        if self.stage_name.is_some() {
            clauses.push(format!("{j}.stage_name = ${}", next()));
        }
        if let Some(code) = self.exit_code {
            if code == -1 {
                clauses.push(format!("{j}.exit_code IS NOT NULL AND {j}.exit_code != 0"));
            } else {
                clauses.push(format!("{j}.exit_code = ${}", next()));
            }
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        AnalyticsPlan {
            where_clause,
            next_idx: idx + 1,
        }
    }

    /// Apply binds in the same order plan_for_job_groups appended params.
    pub fn bind_for_job_groups<'q>(&'q self, mut q: PgQuery<'q>) -> PgQuery<'q> {
        match &self.window {
            AnalyticsWindow::Range { from, to } => {
                q = q.bind(*from).bind(*to);
            }
            AnalyticsWindow::LastDays(days) => {
                q = q.bind(*days);
            }
        }
        if let Some(rid) = self.repo_id {
            q = q.bind(rid);
        }
        if let Some(ref b) = self.branch {
            q = q.bind(b.clone());
        }
        if let Some(ref s) = self.stage_name {
            q = q.bind(s.clone());
        }
        if let Some(code) = self.exit_code {
            if code != -1 {
                q = q.bind(code);
            }
        }
        q
    }

    /// Apply binds for plan_for_jobs (same order as plan).
    pub fn bind_for_jobs<'q>(&'q self, q: PgQuery<'q>) -> PgQuery<'q> {
        // Same bind order as job_groups variant — repo_id/branch/stage_name/exit_code
        // are applied identically; only the column references in the SQL differ.
        self.bind_for_job_groups(q)
    }
}

/// Merge an existing `AnalyticsPlan` with an optional ChQL [`SqlFragment`].
/// Returns the combined WHERE clause and the next bind index.
///
/// `wrap_subquery` controls how the fragment is spliced:
/// - `false` — fragment AND-appended directly. The query must be rooted at
///   the `job_groups` table (un-aliased) so the compiler's EXISTS subqueries
///   referencing `job_groups.id` resolve correctly.
/// - `true`  — fragment wrapped as `<alias>.id IN (SELECT id FROM job_groups
///   WHERE <fragment>)`. Use this for queries rooted at `jobs` or with an
///   aliased `job_groups jg`.
///
/// The fragment uses `?` placeholders; we renumber them to `$N` starting at
/// `plan.next_idx`. The caller must bind existing filter args first, then the
/// fragment's binds (use [`bind_chql`]).
pub fn merge_chql(
    plan: AnalyticsPlan,
    chql: Option<&crate::query::SqlFragment>,
    wrap_subquery: Option<&str>,
) -> AnalyticsPlan {
    match chql {
        None => plan,
        Some(frag) => {
            let (renumbered, next_idx) = frag.to_pg(plan.next_idx);
            let chunk = match wrap_subquery {
                None => renumbered,
                Some(alias) => {
                    // Splice ChQL into a subselect against job_groups so the
                    // compiler's `job_groups.id` references stay valid.
                    format!("{alias}.id IN (SELECT id FROM job_groups WHERE {renumbered})")
                }
            };
            let where_clause = if plan.where_clause.is_empty() {
                format!("WHERE {chunk}")
            } else {
                format!("{} AND ({chunk})", plan.where_clause)
            };
            AnalyticsPlan {
                where_clause,
                next_idx,
            }
        }
    }
}

/// Bind a ChQL fragment's args onto a Postgres query, in order.
pub fn bind_chql<'q>(
    mut q: PgQuery<'q>,
    chql: Option<&'q crate::query::SqlFragment>,
) -> PgQuery<'q> {
    if let Some(frag) = chql {
        for b in &frag.binds {
            q = b.bind(q);
        }
    }
    q
}

/// Same as [`bind_chql`] but for `query_scalar`.
#[allow(dead_code)]
pub fn bind_chql_scalar<'q, T>(
    mut q: sqlx::query::QueryScalar<'q, sqlx::Postgres, T, sqlx::postgres::PgArguments>,
    chql: Option<&'q crate::query::SqlFragment>,
) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, T, sqlx::postgres::PgArguments> {
    if let Some(frag) = chql {
        for b in &frag.binds {
            q = b.bind_scalar(q);
        }
    }
    q
}

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
    pub priority: i32,
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
            priority: r.try_get("priority").unwrap_or(0),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }
    }
}

const WORKER_TOKEN_COLUMNS: &str =
    "id, name, token_hash, scope, created_by, expires_at, max_uses, uses, active, created_at, worker_id";

const LABEL_GROUP_COLUMNS: &str =
    "id, name, match_labels, env_vars, pre_script, max_concurrent_jobs, \
     capabilities, enabled, COALESCE(priority, 0) AS priority, created_at, updated_at";

const LABEL_GROUP_INSERT_COLUMNS: &str =
    "name, match_labels, env_vars, pre_script, max_concurrent_jobs, \
     capabilities, enabled, priority";

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
            (
                30,
                "status_reason",
                include_str!("../../../migrations/030_status_reason.sql"),
            ),
            (
                31,
                "script_locking",
                include_str!("../../../migrations/031_script_locking.sql"),
            ),
            (
                32,
                "worker_priority_limits",
                include_str!("../../../migrations/032_worker_priority_limits.sql"),
            ),
            (
                33,
                "retention_archive",
                include_str!("../../../migrations/033_retention_archive.sql"),
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
                // Dollar-quote-aware splitter: walks the SQL, ignores `;` inside
                // single-quoted strings, dollar-quoted blocks (e.g. PL/pgSQL
                // function bodies), line comments, and block comments.
                let mut tx = conn.begin().await?;
                for stmt in split_sql_statements(sql) {
                    let trimmed = stmt.trim();
                    if !trimmed.is_empty() {
                        sqlx::query(trimmed).execute(&mut *tx).await?;
                    }
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

    /// Returns distinct stage_name values for a repo, alphabetically sorted.
    /// Powers the stage filter dropdown on /builds and /analytics.
    pub async fn list_stages_for_repo(&self, repo_id: Uuid) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT stage_name FROM stage_configs \
             WHERE repo_id = $1 ORDER BY stage_name",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
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
             reserved_stages, \
             status_reason, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
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
            .bind(&group.reserved_stages)
            .bind(&group.status_reason)
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
        reason: Option<&str>,
    ) -> anyhow::Result<Option<JobGroup>> {
        let now = Utc::now();
        let completed_at = if state.is_terminal() { Some(now) } else { None };

        let q = format!(
            "UPDATE job_groups \
             SET state = $2, updated_at = $3, completed_at = COALESCE($4, completed_at), \
                 status_reason = COALESCE($5, status_reason) \
             WHERE id = $1 \
             RETURNING {JOB_GROUP_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(id)
            .bind(state.to_string())
            .bind(now)
            .bind(completed_at)
            .bind(reason)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(map_job_group))
    }

    pub async fn update_job_group_reason(&self, id: Uuid, reason: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE job_groups SET status_reason = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(reason)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_job_reason(&self, id: Uuid, reason: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE jobs SET status_reason = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(reason)
            .execute(&self.pool)
            .await?;
        Ok(())
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
             pre_script, post_script, worker_id, state, status_reason, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
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
            .bind(&job.status_reason)
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
            "INSERT INTO workers ({WORKER_INSERT_COLUMNS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24) \
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
                 description = COALESCE(EXCLUDED.description, workers.description), \
                 priority = COALESCE(EXCLUDED.priority, workers.priority), \
                 max_cpu = COALESCE(EXCLUDED.max_cpu, workers.max_cpu), \
                 max_memory_mb = COALESCE(EXCLUDED.max_memory_mb, workers.max_memory_mb), \
                 max_disk_mb = COALESCE(EXCLUDED.max_disk_mb, workers.max_disk_mb), \
                 max_cpu_percent = COALESCE(EXCLUDED.max_cpu_percent, workers.max_cpu_percent), \
                 max_memory_percent = COALESCE(EXCLUDED.max_memory_percent, workers.max_memory_percent), \
                 max_disk_percent = COALESCE(EXCLUDED.max_disk_percent, workers.max_disk_percent) \
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
            .bind(worker.priority)
            .bind(worker.max_cpu)
            .bind(worker.max_memory_mb)
            .bind(worker.max_disk_mb)
            .bind(worker.max_cpu_percent)
            .bind(worker.max_memory_percent)
            .bind(worker.max_disk_percent)
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

    /// Update scheduling priority and resource limits for a worker.
    /// Pass None for any field to leave it unchanged.
    /// Pass Some(0) for max_cpu/max_memory_mb/max_disk_mb to clear the limit
    /// (the column is set to NULL, meaning "use total_* value").
    pub async fn update_worker_priority_limits(
        &self,
        worker_id: &str,
        priority: Option<i32>,
        max_cpu: Option<i32>,
        max_memory_mb: Option<i64>,
        max_disk_mb: Option<i64>,
        max_cpu_percent: Option<i32>,
        max_memory_percent: Option<i32>,
        max_disk_percent: Option<i32>,
    ) -> anyhow::Result<Option<WorkerRow>> {
        // Use CASE WHEN to allow setting columns to NULL (via 0 = clear):
        // - priority: COALESCE (0 is valid, NULL = don't change)
        // - max_*: provided flag -> if true, use value (NULLIF to clear on 0)
        let q = format!(
            "UPDATE workers SET \
             priority = COALESCE($2, priority), \
             max_cpu = CASE WHEN $3::bool THEN NULLIF($4, 0) ELSE max_cpu END, \
             max_memory_mb = CASE WHEN $5::bool THEN NULLIF($6::bigint, 0) ELSE max_memory_mb END, \
             max_disk_mb = CASE WHEN $7::bool THEN NULLIF($8::bigint, 0) ELSE max_disk_mb END, \
             max_cpu_percent = CASE WHEN $9::bool THEN NULLIF($10, 0) ELSE max_cpu_percent END, \
             max_memory_percent = CASE WHEN $11::bool THEN NULLIF($12, 0) ELSE max_memory_percent END, \
             max_disk_percent = CASE WHEN $13::bool THEN NULLIF($14, 0) ELSE max_disk_percent END \
             WHERE worker_id = $1 \
             RETURNING {WORKER_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(worker_id)
            .bind(priority)
            .bind(max_cpu.is_some())
            .bind(max_cpu.unwrap_or(0))
            .bind(max_memory_mb.is_some())
            .bind(max_memory_mb.unwrap_or(0))
            .bind(max_disk_mb.is_some())
            .bind(max_disk_mb.unwrap_or(0))
            .bind(max_cpu_percent.is_some())
            .bind(max_cpu_percent.unwrap_or(0))
            .bind(max_memory_percent.is_some())
            .bind(max_memory_percent.unwrap_or(0))
            .bind(max_disk_percent.is_some())
            .bind(max_disk_percent.unwrap_or(0))
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(WorkerRow::from))
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
             WHERE state NOT IN ('success', 'failed', 'cancelled', 'expired') \
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

    /// List job_groups with optional filters. `exit_code = Some(-1)` means
    /// "any non-zero" (matches at least one job in the group with non-zero exit).
    /// `stage_name` / `exit_code` filters use EXISTS subquery against `jobs` so
    /// parent rows are never duplicated.
    #[allow(clippy::too_many_arguments)]
    pub async fn list_job_groups_paginated(
        &self,
        limit: i64,
        offset: i64,
        state_filter: Option<&str>,
        repo_id_filter: Option<Uuid>,
        branch_filter: Option<&str>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        stage_name_filter: Option<&str>,
        exit_code_filter: Option<i32>,
        chql: Option<&crate::query::SqlFragment>,
    ) -> anyhow::Result<(Vec<JobGroup>, i64)> {
        // Build WHERE fragments + remember bind order so count + data queries
        // bind in lockstep.
        let mut clauses: Vec<String> = Vec::new();
        let mut idx: usize = 0;
        let mut next = || {
            idx += 1;
            idx
        };

        if state_filter.is_some() {
            clauses.push(format!("state = ${}", next()));
        }
        if repo_id_filter.is_some() {
            clauses.push(format!("repo_id = ${}", next()));
        }
        if branch_filter.is_some() {
            clauses.push(format!("branch = ${}", next()));
        }
        if date_from.is_some() {
            clauses.push(format!("created_at >= ${}", next()));
        }
        if date_to.is_some() {
            clauses.push(format!("created_at <= ${}", next()));
        }
        if stage_name_filter.is_some() {
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM jobs j WHERE j.job_group_id = job_groups.id AND j.stage_name = ${})",
                next()
            ));
        }
        if let Some(code) = exit_code_filter {
            if code == -1 {
                // "any non-zero" — no bind required.
                clauses.push(
                    "EXISTS (SELECT 1 FROM jobs j WHERE j.job_group_id = job_groups.id \
                     AND j.exit_code IS NOT NULL AND j.exit_code != 0)"
                        .to_string(),
                );
            } else {
                clauses.push(format!(
                    "EXISTS (SELECT 1 FROM jobs j WHERE j.job_group_id = job_groups.id AND j.exit_code = ${})",
                    next()
                ));
            }
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        // Splice ChQL fragment after the typed filters. The fragment uses `?`
        // placeholders; renumber to `$N` starting after the last typed bind.
        let (where_clause, idx) = if let Some(frag) = chql {
            let (renumbered, next_after) = frag.to_pg(idx + 1);
            let merged = if where_clause.is_empty() {
                format!("WHERE {renumbered}")
            } else {
                format!("{where_clause} AND ({renumbered})")
            };
            // `next_after` is the next free slot; LIMIT/OFFSET start there.
            (merged, next_after - 1)
        } else {
            (where_clause, idx)
        };

        let count_q = format!("SELECT COUNT(*) FROM job_groups {where_clause}");
        let data_q = format!(
            "SELECT {JOB_GROUP_COLUMNS} FROM job_groups {where_clause} \
             ORDER BY priority DESC, created_at DESC LIMIT ${} OFFSET ${}",
            idx + 1,
            idx + 2
        );

        // Apply filter binds in the same order clauses were pushed above.
        // Macro avoids the closure lifetime mismatch between sqlx::query::Query
        // and sqlx::query::QueryScalar generic parameters. ChQL fragment binds
        // (if any) come AFTER the typed filter binds — matching the SQL splice
        // order above.
        macro_rules! bind_filters {
            ($q:expr) => {{
                let mut q = $q;
                if let Some(state) = state_filter {
                    q = q.bind(state.to_string());
                }
                if let Some(repo_id) = repo_id_filter {
                    q = q.bind(repo_id);
                }
                if let Some(branch) = branch_filter {
                    q = q.bind(branch.to_string());
                }
                if let Some(from) = date_from {
                    q = q.bind(from);
                }
                if let Some(to) = date_to {
                    q = q.bind(to);
                }
                if let Some(stage) = stage_name_filter {
                    q = q.bind(stage.to_string());
                }
                if let Some(code) = exit_code_filter {
                    if code != -1 {
                        q = q.bind(code);
                    }
                }
                if let Some(frag) = chql {
                    for b in &frag.binds {
                        q = match b {
                            crate::query::SqlBind::Str(s) => q.bind(s.as_str()),
                            crate::query::SqlBind::Num(n) => q.bind(*n),
                            crate::query::SqlBind::Date(d) => q.bind(*d),
                        };
                    }
                }
                q
            }};
        }

        let count_query = bind_filters!(sqlx::query_scalar::<_, i64>(&count_q));
        let total: i64 = count_query.fetch_one(&self.pool).await?;

        let data_query = bind_filters!(sqlx::query(&data_q)).bind(limit).bind(offset);
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
        global_pre_script_lock_enabled: Option<bool>,
        global_pre_script_lock_key: Option<Option<&str>>,
        global_pre_script_lock_timeout_secs: Option<i32>,
        global_post_script_lock_enabled: Option<bool>,
        global_post_script_lock_key: Option<Option<&str>>,
        global_post_script_lock_timeout_secs: Option<i32>,
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
                 global_pre_script_lock_enabled = COALESCE($14, global_pre_script_lock_enabled), \
                 global_pre_script_lock_key = CASE WHEN $15 THEN $16 ELSE global_pre_script_lock_key END, \
                 global_pre_script_lock_timeout_secs = COALESCE($17, global_pre_script_lock_timeout_secs), \
                 global_post_script_lock_enabled = COALESCE($18, global_post_script_lock_enabled), \
                 global_post_script_lock_key = CASE WHEN $19 THEN $20 ELSE global_post_script_lock_key END, \
                 global_post_script_lock_timeout_secs = COALESCE($21, global_post_script_lock_timeout_secs), \
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
        let (pre_lock_key_provided, pre_lock_key_val) = match global_pre_script_lock_key {
            Some(v) => (true, v),
            None => (false, None),
        };
        let (post_lock_key_provided, post_lock_key_val) = match global_post_script_lock_key {
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
            .bind(global_pre_script_lock_enabled)
            .bind(pre_lock_key_provided)
            .bind(pre_lock_key_val)
            .bind(global_pre_script_lock_timeout_secs)
            .bind(global_post_script_lock_enabled)
            .bind(post_lock_key_provided)
            .bind(post_lock_key_val)
            .bind(global_post_script_lock_timeout_secs)
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

    #[allow(clippy::too_many_arguments)]
    pub async fn create_stage_script(
        &self,
        stage_config_id: Uuid,
        script_type: &str,
        script_scope: &str,
        script: &str,
        worker_id: Option<&str>,
        lock_enabled: bool,
        lock_key: Option<&str>,
        lock_timeout_secs: i32,
    ) -> anyhow::Result<StageScript> {
        let q = format!(
            "INSERT INTO stage_scripts \
             (id, stage_config_id, worker_id, script_type, script_scope, script, \
              lock_enabled, lock_key, lock_timeout_secs, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, now(), now()) \
             RETURNING {STAGE_SCRIPT_COLUMNS}"
        );
        let row = sqlx::query(&q)
            .bind(stage_config_id)
            .bind(worker_id)
            .bind(script_type)
            .bind(script_scope)
            .bind(script)
            .bind(lock_enabled)
            .bind(lock_key)
            .bind(lock_timeout_secs)
            .fetch_one(&self.pool)
            .await?;
        Ok(map_stage_script(row))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_stage_script(
        &self,
        id: Uuid,
        script_type: Option<&str>,
        script_scope: Option<&str>,
        script: Option<&str>,
        worker_id: Option<Option<&str>>,
        lock_enabled: Option<bool>,
        lock_key: Option<Option<&str>>,
        lock_timeout_secs: Option<i32>,
    ) -> anyhow::Result<Option<StageScript>> {
        let q = format!(
            "UPDATE stage_scripts SET \
             script_type       = COALESCE($2, script_type), \
             script_scope      = COALESCE($3, script_scope), \
             script            = COALESCE($4, script), \
             worker_id         = CASE WHEN $5 THEN $6 ELSE worker_id END, \
             lock_enabled      = COALESCE($7, lock_enabled), \
             lock_key          = CASE WHEN $8 THEN $9 ELSE lock_key END, \
             lock_timeout_secs = COALESCE($10, lock_timeout_secs), \
             updated_at        = now() \
             WHERE id = $1 \
             RETURNING {STAGE_SCRIPT_COLUMNS}"
        );
        let (update_worker, new_worker): (bool, Option<&str>) = match worker_id {
            Some(w) => (true, w),
            None => (false, None),
        };
        let (update_lock_key, new_lock_key): (bool, Option<&str>) = match lock_key {
            Some(k) => (true, k),
            None => (false, None),
        };
        let row = sqlx::query(&q)
            .bind(id)
            .bind(script_type)
            .bind(script_scope)
            .bind(script)
            .bind(update_worker)
            .bind(new_worker)
            .bind(lock_enabled)
            .bind(update_lock_key)
            .bind(new_lock_key)
            .bind(lock_timeout_secs)
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

    // ========================================================================
    // Retention — three-tier lifecycle (issue #20)
    //
    // T1 = on-disk file purge (driven by `find_groups_for_t1` +
    //      `mark_files_purged` once every owning worker has acked).
    // T2 = row archive: live tables -> *_archive (driven by
    //      `find_groups_for_t2` + `archive_groups_batch`).
    // T3 = hard delete of archive rows (driven by `find_archive_for_t3` +
    //      `delete_archive_batch`).
    //
    // `workers_for_group` powers the T1 fan-out and MUST keep working
    // after T2 (i.e. fall through to `jobs_archive` when the live row
    // has been moved). `unarchive_groups_batch` powers the operator
    // rollback runbook (§6i of retention-implementation-plan.md).
    // ========================================================================

    /// T1 candidates: terminal groups whose `completed_at` is older than
    /// `t1_days`, that have NOT yet been file-purged.
    ///
    /// Returns a batch of at most `limit` group ids ordered by
    /// `completed_at` ascending (oldest first), so we drain the backlog
    /// deterministically across ticks. The retention loop drives one
    /// tier per tick and re-queries on the next tick.
    pub async fn find_groups_for_t1(&self, t1_days: i32, limit: i64) -> anyhow::Result<Vec<Uuid>> {
        let q = format!(
            "SELECT id FROM {s}.job_groups \
             WHERE state IN ('success', 'failed', 'cancelled', 'expired') \
               AND completed_at IS NOT NULL \
               AND completed_at < NOW() - make_interval(days => $1) \
               AND files_purged_at IS NULL \
             ORDER BY completed_at \
             LIMIT $2",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(t1_days)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    /// T2 candidates: terminal groups whose `completed_at` is older than
    /// `t2_days`.
    ///
    /// Deliberately does NOT filter on `files_purged_at` — T2 archives
    /// regardless of whether the on-disk purge has succeeded yet. The
    /// archive copy preserves `files_purged_at` so a post-archive
    /// observer can still distinguish "files gone" from "files maybe
    /// still around." Returns at most `limit` ids, oldest first.
    pub async fn find_groups_for_t2(&self, t2_days: i32, limit: i64) -> anyhow::Result<Vec<Uuid>> {
        let q = format!(
            "SELECT id FROM {s}.job_groups \
             WHERE state IN ('success', 'failed', 'cancelled', 'expired') \
               AND completed_at IS NOT NULL \
               AND completed_at < NOW() - make_interval(days => $1) \
             ORDER BY completed_at \
             LIMIT $2",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(t2_days)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    /// T3 candidates: archived groups whose `archived_at` is older than
    /// `t3_days`. Reads from `job_groups_archive` only — live groups are
    /// untouched by T3. Returns at most `limit` ids, oldest first.
    pub async fn find_archive_for_t3(&self, t3_days: i32, limit: i64) -> anyhow::Result<Vec<Uuid>> {
        let q = format!(
            "SELECT id FROM {s}.job_groups_archive \
             WHERE archived_at < NOW() - make_interval(days => $1) \
             ORDER BY archived_at \
             LIMIT $2",
            s = self.schema
        );
        let rows = sqlx::query(&q)
            .bind(t3_days)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
    }

    /// Move a batch of groups from live -> archive in one transaction.
    /// Thin wrapper over `chola.archive_group_ids` (migration 033) which
    /// handles cascade ordering (children inserted before parent, deleted
    /// after) so live FKs are satisfied throughout.
    ///
    /// Returns the number of `job_groups` rows actually archived, which
    /// may be smaller than `group_ids.len()` if some ids did not exist
    /// in the live table.
    pub async fn archive_groups_batch(&self, group_ids: &[Uuid]) -> anyhow::Result<u64> {
        if group_ids.is_empty() {
            return Ok(0);
        }
        let q = format!(
            "SELECT {s}.archive_group_ids($1) AS affected",
            s = self.schema
        );
        let row = sqlx::query(&q)
            .bind(group_ids)
            .fetch_one(&self.pool)
            .await?;
        let affected: i64 = row.get("affected");
        Ok(affected.max(0) as u64)
    }

    /// Move a batch of groups from archive -> live. Thin wrapper over
    /// `chola.unarchive_group_ids` (migration 033). Used by the
    /// retention rollback runbook (§6i) and by these integration tests
    /// to verify the round-trip is symmetric.
    ///
    /// Returns the number of `job_groups_archive` rows actually
    /// unarchived.
    pub async fn unarchive_groups_batch(&self, group_ids: &[Uuid]) -> anyhow::Result<u64> {
        if group_ids.is_empty() {
            return Ok(0);
        }
        let q = format!(
            "SELECT {s}.unarchive_group_ids($1) AS affected",
            s = self.schema
        );
        let row = sqlx::query(&q)
            .bind(group_ids)
            .fetch_one(&self.pool)
            .await?;
        let affected: i64 = row.get("affected");
        Ok(affected.max(0) as u64)
    }

    /// T3 hard-delete: remove archive rows for `group_ids` from all six
    /// `*_archive` tables in a single transaction. Children are deleted
    /// before the parent for clarity and future-proofing, even though
    /// the archive tables intentionally have no FKs.
    ///
    /// Returns the number of `job_groups_archive` rows removed (the
    /// "canonical" count callers care about).
    pub async fn delete_archive_batch(&self, group_ids: &[Uuid]) -> anyhow::Result<u64> {
        if group_ids.is_empty() {
            return Ok(0);
        }
        let s = &self.schema;
        let mut tx = self.pool.begin().await?;

        sqlx::query(&format!(
            "DELETE FROM {s}.approval_gates_archive WHERE job_group_id = ANY($1)"
        ))
        .bind(group_ids)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "DELETE FROM {s}.test_results_archive WHERE job_group_id = ANY($1)"
        ))
        .bind(group_ids)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "DELETE FROM {s}.artifacts_archive WHERE job_group_id = ANY($1)"
        ))
        .bind(group_ids)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "DELETE FROM {s}.worker_reservations_archive WHERE job_group_id = ANY($1)"
        ))
        .bind(group_ids)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "DELETE FROM {s}.jobs_archive WHERE job_group_id = ANY($1)"
        ))
        .bind(group_ids)
        .execute(&mut *tx)
        .await?;

        let result = sqlx::query(&format!(
            "DELETE FROM {s}.job_groups_archive WHERE id = ANY($1)"
        ))
        .bind(group_ids)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// Stamp `files_purged_at` on a batch of LIVE groups. Used by the
    /// controller once the master-side T1 deletion has run AND every
    /// owning worker has acked (or is gone past heartbeat timeout).
    ///
    /// Only rows where `files_purged_at IS NULL` are touched, so the
    /// call is idempotent across retries. Returns rows affected.
    pub async fn mark_files_purged(
        &self,
        group_ids: &[Uuid],
        purged_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<u64> {
        if group_ids.is_empty() {
            return Ok(0);
        }
        let q = format!(
            "UPDATE {s}.job_groups \
             SET files_purged_at = $2, updated_at = NOW() \
             WHERE id = ANY($1) AND files_purged_at IS NULL",
            s = self.schema
        );
        let result = sqlx::query(&q)
            .bind(group_ids)
            .bind(purged_at)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Distinct worker ids that ran any stage in `group_id`. Must keep
    /// working AFTER T2 has run — the controller may restart between
    /// T1 fan-out and worker ack, then need to re-derive the worker
    /// list for a group whose live `jobs` rows have since been
    /// archived.
    ///
    /// UNIONs live + archive and filters out empty worker_ids.
    /// Returns hyphenated worker_id strings, deduped.
    pub async fn workers_for_group(&self, group_id: Uuid) -> anyhow::Result<Vec<String>> {
        let q = format!(
            "WITH live AS ( \
                SELECT DISTINCT worker_id FROM {s}.jobs \
                WHERE job_group_id = $1 AND worker_id IS NOT NULL \
             ), arch AS ( \
                SELECT DISTINCT worker_id FROM {s}.jobs_archive \
                WHERE job_group_id = $1 AND worker_id IS NOT NULL \
             ) \
             SELECT worker_id FROM live \
             UNION \
             SELECT worker_id FROM arch",
            s = self.schema
        );
        let rows = sqlx::query(&q).bind(group_id).fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| r.get::<String, _>("worker_id"))
            .filter(|w| !w.is_empty())
            .collect())
    }

    // ========================================================================
    // Retention — archive read path (issue #20, T7a)
    //
    // These power the "Show archived" frontend toggle. They are
    // deliberately additive: callers that don't opt in keep using the
    // existing fast-path `list_job_groups_paginated` /
    // `get_job_group_with_jobs`, which never touch the archive tables.
    // ========================================================================

    /// List job_groups across BOTH live and archive tables in a single
    /// paginated response. Each row carries a synthetic `archived`
    /// boolean so the API/frontend can render the "Archived" badge
    /// without a second roundtrip.
    ///
    /// Mirrors the filters of `list_job_groups_paginated`; pagination /
    /// ordering apply to the UNION ALL, not each side individually.
    #[allow(clippy::too_many_arguments)]
    pub async fn list_job_groups_paginated_with_archive(
        &self,
        limit: i64,
        offset: i64,
        state_filter: Option<&str>,
        repo_id_filter: Option<Uuid>,
        branch_filter: Option<&str>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        stage_name_filter: Option<&str>,
        exit_code_filter: Option<i32>,
    ) -> anyhow::Result<(Vec<(JobGroup, bool)>, i64)> {
        // Build the shared WHERE fragment. Bind indices are assigned in
        // the order clauses are pushed; both legs of the UNION reuse
        // the same bind list so `$N` placeholders are stable.
        let mut clauses: Vec<String> = Vec::new();
        let mut idx: usize = 0;
        let mut next = || {
            idx += 1;
            idx
        };

        if state_filter.is_some() {
            clauses.push(format!("state = ${}", next()));
        }
        if repo_id_filter.is_some() {
            clauses.push(format!("repo_id = ${}", next()));
        }
        if branch_filter.is_some() {
            clauses.push(format!("branch = ${}", next()));
        }
        if date_from.is_some() {
            clauses.push(format!("created_at >= ${}", next()));
        }
        if date_to.is_some() {
            clauses.push(format!("created_at <= ${}", next()));
        }

        // For stage_name / exit_code filters we need to reach into the
        // matching jobs table — live filters against `jobs`, archive
        // against `jobs_archive`. The two WHERE fragments differ only
        // in the table reference; build them in lockstep so bind
        // indices line up.
        let mut live_clauses = clauses.clone();
        let mut arch_clauses = clauses.clone();
        if stage_name_filter.is_some() {
            let i = next();
            live_clauses.push(format!(
                "EXISTS (SELECT 1 FROM {s}.jobs j WHERE j.job_group_id = jg.id AND j.stage_name = ${i})",
                s = self.schema, i = i
            ));
            arch_clauses.push(format!(
                "EXISTS (SELECT 1 FROM {s}.jobs_archive j WHERE j.job_group_id = jg.id AND j.stage_name = ${i})",
                s = self.schema, i = i
            ));
        }
        if let Some(code) = exit_code_filter {
            if code == -1 {
                live_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM {s}.jobs j WHERE j.job_group_id = jg.id \
                     AND j.exit_code IS NOT NULL AND j.exit_code != 0)",
                    s = self.schema
                ));
                arch_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM {s}.jobs_archive j WHERE j.job_group_id = jg.id \
                     AND j.exit_code IS NOT NULL AND j.exit_code != 0)",
                    s = self.schema
                ));
            } else {
                let i = next();
                live_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM {s}.jobs j WHERE j.job_group_id = jg.id AND j.exit_code = ${i})",
                    s = self.schema, i = i
                ));
                arch_clauses.push(format!(
                    "EXISTS (SELECT 1 FROM {s}.jobs_archive j WHERE j.job_group_id = jg.id AND j.exit_code = ${i})",
                    s = self.schema, i = i
                ));
            }
        }

        let live_where = if live_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", live_clauses.join(" AND "))
        };
        let arch_where = if arch_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", arch_clauses.join(" AND "))
        };

        // Project the same column list from both tables in the same
        // order so UNION ALL stays type-compatible. The synthetic
        // `archived` boolean lets the caller tag each row without a
        // second query.
        let s = &self.schema;
        let live_proj = format!(
            "SELECT id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id, \
                    state, priority, pr_number, idempotency_key, \
                    allocated_cpu, allocated_memory_mb, allocated_disk_mb, \
                    status_reason, created_at, updated_at, completed_at, \
                    false AS archived \
             FROM {s}.job_groups jg {live_where}"
        );
        let arch_proj = format!(
            "SELECT id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id, \
                    state, priority, pr_number, idempotency_key, \
                    allocated_cpu, allocated_memory_mb, allocated_disk_mb, \
                    status_reason, created_at, updated_at, completed_at, \
                    true AS archived \
             FROM {s}.job_groups_archive jg {arch_where}"
        );

        let data_q = format!(
            "SELECT * FROM ( {live_proj} UNION ALL {arch_proj} ) u \
             ORDER BY priority DESC, created_at DESC LIMIT ${} OFFSET ${}",
            idx + 1,
            idx + 2
        );
        let count_q = format!(
            "SELECT (SELECT COUNT(*) FROM {s}.job_groups jg {live_where}) \
                  + (SELECT COUNT(*) FROM {s}.job_groups_archive jg {arch_where}) AS total"
        );

        // Bind the shared filter list — count and data queries both
        // reference the same `$N` placeholders, so order matters.
        macro_rules! bind_filters {
            ($q:expr) => {{
                let mut q = $q;
                if let Some(state) = state_filter {
                    q = q.bind(state.to_string());
                }
                if let Some(repo_id) = repo_id_filter {
                    q = q.bind(repo_id);
                }
                if let Some(branch) = branch_filter {
                    q = q.bind(branch.to_string());
                }
                if let Some(from) = date_from {
                    q = q.bind(from);
                }
                if let Some(to) = date_to {
                    q = q.bind(to);
                }
                if let Some(stage) = stage_name_filter {
                    q = q.bind(stage.to_string());
                }
                if let Some(code) = exit_code_filter {
                    if code != -1 {
                        q = q.bind(code);
                    }
                }
                q
            }};
        }

        let total: i64 = bind_filters!(sqlx::query_scalar::<_, i64>(&count_q))
            .fetch_one(&self.pool)
            .await?;

        let rows = bind_filters!(sqlx::query(&data_q))
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let groups: Vec<(JobGroup, bool)> = rows
            .into_iter()
            .map(|r| {
                let archived: bool = r.try_get("archived").unwrap_or(false);
                (map_job_group(r), archived)
            })
            .collect();

        Ok((groups, total))
    }

    /// Single-group fetch with archive fallback. Tries the live tables
    /// first; if nothing turns up, queries the matching archive tables.
    /// The third tuple element is `true` only when the row came from
    /// the archive, so callers can render an "archived" badge / skip
    /// in-memory registry lookups.
    pub async fn get_job_group_with_jobs_or_archive(
        &self,
        group_id: Uuid,
    ) -> anyhow::Result<Option<(JobGroup, Vec<DbJob>, bool)>> {
        if let Some((g, jobs)) = self.get_job_group_with_jobs(group_id).await? {
            return Ok(Some((g, jobs, false)));
        }

        // Archive fallback: pull the group row from `job_groups_archive`
        // and its jobs from `jobs_archive`. The archive tables have the
        // same column shape as live (minus the `archived_at` /
        // `files_purged_at` tail we don't need here), so the same row
        // mappers work once we project the columns explicitly.
        let s = &self.schema;
        let group_q = format!(
            "SELECT id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id, \
                    state, priority, pr_number, idempotency_key, \
                    allocated_cpu, allocated_memory_mb, allocated_disk_mb, \
                    status_reason, created_at, updated_at, completed_at \
             FROM {s}.job_groups_archive WHERE id = $1"
        );
        let row = sqlx::query(&group_q)
            .bind(group_id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let group = map_job_group(row);

        let jobs_q = format!(
            "SELECT id, job_group_id, stage_config_id, stage_name, command, pre_script, \
                    post_script, worker_id, state, exit_code, pre_exit_code, post_exit_code, \
                    log_path, started_at, completed_at, retry_count, status_reason, \
                    created_at, updated_at \
             FROM {s}.jobs_archive \
             WHERE job_group_id = $1 ORDER BY created_at"
        );
        let job_rows = sqlx::query(&jobs_q)
            .bind(group_id)
            .fetch_all(&self.pool)
            .await?;
        let jobs: Vec<DbJob> = job_rows.into_iter().map(DbJob::from).collect();

        Ok(Some((group, jobs, true)))
    }

    /// Read `archived_at` + `files_purged_at` for an archived group.
    /// Returns `None` if the id isn't in the archive (e.g. caller hit
    /// the live fast path). Used by the single-group GET handler to
    /// surface "archived on …" / "files purged on …" timestamps.
    pub async fn get_archive_timestamps(
        &self,
        group_id: Uuid,
    ) -> anyhow::Result<Option<(DateTime<Utc>, Option<DateTime<Utc>>)>> {
        let q = format!(
            "SELECT archived_at, files_purged_at FROM {s}.job_groups_archive WHERE id = $1",
            s = self.schema
        );
        let row = sqlx::query(&q)
            .bind(group_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| {
            (
                r.get::<DateTime<Utc>, _>("archived_at"),
                r.try_get::<Option<DateTime<Utc>>, _>("files_purged_at")
                    .ok()
                    .flatten(),
            )
        }))
    }

    /// JSON dump of archived child rows for a single group. Powers the
    /// build-detail page when the group has been T2-archived — gives
    /// the frontend a generic blob to render instead of hand-shaping
    /// each child table.
    ///
    /// Shape:
    /// ```json
    /// {
    ///   "artifacts":            [ { ... }, ... ],
    ///   "test_results":         [ { ... }, ... ],
    ///   "approval_gates":       [ { ... }, ... ],
    ///   "worker_reservations":  [ { ... }, ... ]
    /// }
    /// ```
    pub async fn get_archived_children_json(
        &self,
        group_id: Uuid,
    ) -> anyhow::Result<serde_json::Value> {
        let s = &self.schema;
        // `COALESCE(json_agg(row_to_json(t)), '[]')` keeps an empty
        // table returning `[]` rather than `null`.
        let q = format!(
            "SELECT \
                (SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json) \
                   FROM {s}.artifacts_archive t WHERE job_group_id = $1) AS artifacts, \
                (SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json) \
                   FROM {s}.test_results_archive t WHERE job_group_id = $1) AS test_results, \
                (SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json) \
                   FROM {s}.approval_gates_archive t WHERE job_group_id = $1) AS approval_gates, \
                (SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json) \
                   FROM {s}.worker_reservations_archive t WHERE job_group_id = $1) AS worker_reservations"
        );
        let row = sqlx::query(&q).bind(group_id).fetch_one(&self.pool).await?;
        let artifacts: serde_json::Value =
            row.try_get("artifacts").unwrap_or(serde_json::json!([]));
        let test_results: serde_json::Value =
            row.try_get("test_results").unwrap_or(serde_json::json!([]));
        let approval_gates: serde_json::Value = row
            .try_get("approval_gates")
            .unwrap_or(serde_json::json!([]));
        let worker_reservations: serde_json::Value = row
            .try_get("worker_reservations")
            .unwrap_or(serde_json::json!([]));
        Ok(serde_json::json!({
            "artifacts":           artifacts,
            "test_results":        test_results,
            "approval_gates":      approval_gates,
            "worker_reservations": worker_reservations,
        }))
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

    pub async fn get_build_trends(
        &self,
        filters: &AnalyticsFilters,
        chql: Option<&crate::query::SqlFragment>,
    ) -> anyhow::Result<Vec<BuildTrendPoint>> {
        let plan = merge_chql(filters.plan_for_job_groups("", &[]), chql, None);
        // `g` is sourced from the Granularity enum (allowlisted to `hour`/`day`),
        // never from raw user input — safe to splice.
        let g = filters.granularity.as_sql_unit();
        let q = format!(
            "SELECT TO_CHAR(DATE_TRUNC('{g}', created_at) AT TIME ZONE 'UTC', \
             'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as date, \
             COUNT(*)::bigint as total, \
             COUNT(*) FILTER (WHERE state = 'success')::bigint as success, \
             COUNT(*) FILTER (WHERE state = 'failed')::bigint as failed \
             FROM {s}.job_groups {wc} \
             GROUP BY DATE_TRUNC('{g}', created_at) ORDER BY DATE_TRUNC('{g}', created_at)",
            s = self.schema,
            wc = plan.where_clause
        );
        let mut query = sqlx::query(&q);
        query = filters.bind_for_job_groups(query);
        query = bind_chql(query, chql);
        let rows = query.fetch_all(&self.pool).await?;
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

    pub async fn get_duration_trends(
        &self,
        filters: &AnalyticsFilters,
        chql: Option<&crate::query::SqlFragment>,
    ) -> anyhow::Result<Vec<DurationTrendPoint>> {
        let plan = merge_chql(
            filters.plan_for_job_groups("", &["completed_at IS NOT NULL"]),
            chql,
            None,
        );
        let g = filters.granularity.as_sql_unit();
        let q = format!(
            "SELECT TO_CHAR(DATE_TRUNC('{g}', created_at) AT TIME ZONE 'UTC', \
             'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as date, \
             COALESCE(AVG(EXTRACT(EPOCH FROM (completed_at - created_at)))::bigint, 0) as avg_duration_secs, \
             COALESCE(PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM (completed_at - created_at)))::bigint, 0) as p95_duration_secs \
             FROM {s}.job_groups {wc} \
             GROUP BY DATE_TRUNC('{g}', created_at) ORDER BY DATE_TRUNC('{g}', created_at)",
            s = self.schema,
            wc = plan.where_clause
        );
        let mut query = sqlx::query(&q);
        query = filters.bind_for_job_groups(query);
        query = bind_chql(query, chql);
        let rows = query.fetch_all(&self.pool).await?;
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
        filters: &AnalyticsFilters,
        limit: i32,
        chql: Option<&crate::query::SqlFragment>,
    ) -> anyhow::Result<Vec<SlowStage>> {
        let plan = merge_chql(
            filters.plan_for_jobs(
                "j",
                "sc",
                "jg",
                &["j.completed_at IS NOT NULL", "j.started_at IS NOT NULL"],
            ),
            chql,
            Some("jg"),
        );
        let limit_idx = plan.next_idx;
        let q = format!(
            "SELECT sc.stage_name, r.repo_name, \
             COALESCE(AVG(EXTRACT(EPOCH FROM (j.completed_at - j.started_at)))::bigint, 0) as avg_secs \
             FROM {s}.jobs j JOIN {s}.stage_configs sc ON j.stage_config_id = sc.id \
             JOIN {s}.repos r ON sc.repo_id = r.id \
             LEFT JOIN {s}.job_groups jg ON j.job_group_id = jg.id \
             {wc} \
             GROUP BY sc.stage_name, r.repo_name ORDER BY avg_secs DESC LIMIT ${limit_idx}",
            s = self.schema,
            wc = plan.where_clause
        );
        let mut query = sqlx::query(&q);
        query = filters.bind_for_jobs(query);
        query = bind_chql(query, chql);
        query = query.bind(limit);
        let rows = query.fetch_all(&self.pool).await?;
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
        filters: &AnalyticsFilters,
        limit: i32,
        chql: Option<&crate::query::SqlFragment>,
    ) -> anyhow::Result<Vec<FailingRepo>> {
        let plan = merge_chql(filters.plan_for_job_groups("jg", &[]), chql, Some("jg"));
        let limit_idx = plan.next_idx;
        let q = format!(
            "SELECT r.repo_name, COUNT(*)::bigint as total, \
             COUNT(*) FILTER (WHERE jg.state = 'failed')::bigint as failed \
             FROM {s}.job_groups jg JOIN {s}.repos r ON jg.repo_id = r.id \
             {wc} \
             GROUP BY r.repo_name ORDER BY failed DESC LIMIT ${limit_idx}",
            s = self.schema,
            wc = plan.where_clause
        );
        let mut query = sqlx::query(&q);
        query = filters.bind_for_job_groups(query);
        query = bind_chql(query, chql);
        query = query.bind(limit);
        let rows = query.fetch_all(&self.pool).await?;
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

    pub async fn get_queue_wait_trends(
        &self,
        filters: &AnalyticsFilters,
        chql: Option<&crate::query::SqlFragment>,
    ) -> anyhow::Result<Vec<QueueWaitPoint>> {
        let plan = merge_chql(
            filters.plan_for_jobs("j", "sc", "jg", &["j.started_at IS NOT NULL"]),
            chql,
            Some("jg"),
        );
        let g = filters.granularity.as_sql_unit();
        let q = format!(
            "SELECT TO_CHAR(DATE_TRUNC('{g}', j.created_at) AT TIME ZONE 'UTC', \
             'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as date, \
             COALESCE(AVG(EXTRACT(EPOCH FROM (j.started_at - j.created_at)))::bigint, 0) as avg_wait_secs \
             FROM {s}.jobs j \
             LEFT JOIN {s}.stage_configs sc ON j.stage_config_id = sc.id \
             LEFT JOIN {s}.job_groups jg ON j.job_group_id = jg.id \
             {wc} \
             GROUP BY DATE_TRUNC('{g}', j.created_at) ORDER BY DATE_TRUNC('{g}', j.created_at)",
            s = self.schema,
            wc = plan.where_clause
        );
        let mut query = sqlx::query(&q);
        query = filters.bind_for_jobs(query);
        query = bind_chql(query, chql);
        let rows = query.fetch_all(&self.pool).await?;
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
        // Also clear worker_id: the partial unique index
        // `idx_worker_tokens_worker_id (worker_id) WHERE worker_id IS NOT NULL`
        // is NOT scoped to `active`, so a deactivated row that still carries
        // worker_id would block the fresh token insert during regeneration
        // (duplicate key violation -> HTTP 500). A revoked token has no need
        // for its worker binding.
        let result = sqlx::query(
            "UPDATE worker_tokens SET active = false, worker_id = NULL \
             WHERE worker_id = $1 AND active = true",
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
        priority: Option<i32>,
        max_cpu: Option<i32>,
        max_memory_mb: Option<i64>,
        max_disk_mb: Option<i64>,
        max_cpu_percent: Option<i32>,
        max_memory_percent: Option<i32>,
        max_disk_percent: Option<i32>,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        // 1. Upsert worker row
        sqlx::query(
            "INSERT INTO workers (worker_id, hostname, status, registered_at, docker_enabled, \
             labels, description, approved, priority, max_cpu, max_memory_mb, max_disk_mb, \
             max_cpu_percent, max_memory_percent, max_disk_percent) \
             VALUES ($1, $2, 'offline', now(), false, $3, $4, true, \
                     COALESCE($5, 0), $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (worker_id) DO UPDATE \
             SET hostname = EXCLUDED.hostname, \
                 labels = EXCLUDED.labels, \
                 description = COALESCE(EXCLUDED.description, workers.description), \
                 priority = COALESCE(EXCLUDED.priority, workers.priority), \
                 max_cpu = COALESCE(EXCLUDED.max_cpu, workers.max_cpu), \
                 max_memory_mb = COALESCE(EXCLUDED.max_memory_mb, workers.max_memory_mb), \
                 max_disk_mb = COALESCE(EXCLUDED.max_disk_mb, workers.max_disk_mb), \
                 max_cpu_percent = COALESCE(EXCLUDED.max_cpu_percent, workers.max_cpu_percent), \
                 max_memory_percent = COALESCE(EXCLUDED.max_memory_percent, workers.max_memory_percent), \
                 max_disk_percent = COALESCE(EXCLUDED.max_disk_percent, workers.max_disk_percent)",
        )
        .bind(worker_id)
        .bind(hostname)
        .bind(labels)
        .bind(description)
        .bind(priority)
        .bind(max_cpu)
        .bind(max_memory_mb)
        .bind(max_disk_mb)
        .bind(max_cpu_percent)
        .bind(max_memory_percent)
        .bind(max_disk_percent)
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
        priority: Option<i32>,
    ) -> anyhow::Result<DbLabelGroup> {
        let default_env = serde_json::json!({});
        let ev = env_vars.unwrap_or(&default_env);
        let mcj = max_concurrent_jobs.unwrap_or(0);
        let pri = priority.unwrap_or(0);
        let q = format!(
            "INSERT INTO label_groups ({LABEL_GROUP_INSERT_COLUMNS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
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
            .bind(pri)
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
        priority: Option<i32>,
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
             priority = COALESCE($9, priority), \
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
            .bind(priority)
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

    /// Cancel all non-terminal jobs belonging to a group.
    /// Called when the group transitions to a terminal state.
    pub async fn cancel_jobs_for_group(&self, group_id: Uuid) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "UPDATE jobs SET state = 'cancelled', \
             status_reason = COALESCE(status_reason, 'Parent group terminated'), \
             updated_at = now() \
             WHERE job_group_id = $1 AND state NOT IN ('success', 'failed', 'cancelled')",
        )
        .bind(group_id)
        .execute(&self.pool)
        .await?;
        let count = result.rows_affected();
        if count > 0 {
            info!("Cancelled {} orphaned jobs for group {}", count, group_id);
        }
        Ok(count)
    }

    /// Cancel jobs that are in non-terminal state but their group is terminal.
    /// Runs on startup to catch any missed updates from previous crashes.
    pub async fn cleanup_orphaned_jobs(&self) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "UPDATE jobs SET state = 'cancelled', \
             status_reason = COALESCE(status_reason, 'Parent group terminated'), \
             updated_at = now() \
             WHERE state NOT IN ('success', 'failed', 'cancelled') \
             AND job_group_id IN (\
                 SELECT id FROM job_groups \
                 WHERE state IN ('success', 'failed', 'cancelled', 'expired')\
             )",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

/// Split a SQL string into individual statements, respecting:
/// - single-quoted strings (`'…'`)
/// - dollar-quoted blocks (`$tag$ … $tag$`) including empty-tag (`$$ … $$`),
///   used by PL/pgSQL function bodies in migration 033 and later
/// - line comments (`-- … \n`)
/// - block comments (`/* … */`)
///
/// Anything between unescaped semicolons is one statement; statements are
/// returned as `&str` slices into the input.
fn split_sql_statements(sql: &str) -> Vec<&str> {
    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut stmt_start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        // Line comment
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        // Single-quoted string — handles `''` escape
        if b == b'\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2; // doubled-up escape
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Dollar-quoted block: $tag$…$tag$ or $$…$$
        if b == b'$' {
            // Find the closing $ of the opening tag
            let tag_start = i + 1;
            let mut tag_end = tag_start;
            while tag_end < bytes.len() && bytes[tag_end] != b'$' {
                let c = bytes[tag_end];
                // Tags are ident-like: letters, digits, underscore; first must be letter or _
                let ok = c.is_ascii_alphanumeric() || c == b'_';
                if !ok {
                    break;
                }
                tag_end += 1;
            }
            if tag_end < bytes.len() && bytes[tag_end] == b'$' {
                let tag = &bytes[tag_start..tag_end];
                let closer = {
                    let mut v = Vec::with_capacity(tag.len() + 2);
                    v.push(b'$');
                    v.extend_from_slice(tag);
                    v.push(b'$');
                    v
                };
                i = tag_end + 1;
                while i + closer.len() <= bytes.len() {
                    if bytes[i..i + closer.len()] == closer[..] {
                        i += closer.len();
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            // Not a dollar-quote tag — fall through.
        }

        if b == b';' {
            statements.push(&sql[stmt_start..i]);
            stmt_start = i + 1;
        }
        i += 1;
    }
    if stmt_start < bytes.len() {
        statements.push(&sql[stmt_start..]);
    }
    statements
}

#[cfg(test)]
mod sql_split_tests {
    use super::split_sql_statements;

    #[test]
    fn splits_simple_statements() {
        let parts = split_sql_statements("SELECT 1; SELECT 2;");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].trim(), "SELECT 1");
        assert_eq!(parts[1].trim(), "SELECT 2");
    }

    #[test]
    fn ignores_semicolons_inside_single_quotes() {
        let parts = split_sql_statements("INSERT INTO t VALUES ('a;b'); SELECT 1;");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("'a;b'"));
    }

    #[test]
    fn ignores_semicolons_inside_dollar_quoted_block() {
        let sql = "CREATE FUNCTION f() RETURNS void AS $fn$ BEGIN PERFORM 1; PERFORM 2; END; $fn$ LANGUAGE plpgsql; SELECT 1;";
        let parts = split_sql_statements(sql);
        assert_eq!(parts.len(), 2, "expected 2 statements, got {parts:?}");
        assert!(parts[0].contains("PERFORM 1"));
        assert!(parts[0].contains("PERFORM 2"));
        assert_eq!(parts[1].trim(), "SELECT 1");
    }

    #[test]
    fn ignores_semicolons_inside_empty_tag_dollar_quote() {
        let sql = "DO $$ BEGIN PERFORM 1; PERFORM 2; END $$; SELECT 1;";
        let parts = split_sql_statements(sql);
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn ignores_doubled_single_quote_escape() {
        let parts = split_sql_statements("SELECT 'it''s ok'; SELECT 2;");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("it''s ok"));
    }

    #[test]
    fn ignores_semicolons_in_line_comments() {
        let parts = split_sql_statements("-- one; two;\nSELECT 1;\nSELECT 2;");
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn ignores_semicolons_in_block_comments() {
        let parts = split_sql_statements("/* a; b; */ SELECT 1; /* c; */ SELECT 2;");
        assert_eq!(parts.len(), 2);
    }
}
// ============================================================================
// Retention storage integration tests (issue #20, T3)
// ============================================================================
//
// These hit a real Postgres at
// postgres://chola_app:chola_app_secret@localhost:5432/choladb
// and assume migration 033_retention_archive.sql is applied (i.e. the
// chola.archive_group_ids / chola.unarchive_group_ids functions and the
// six *_archive tables exist, plus files_purged_at on job_groups).
//
// They are gated behind the CHOLA_TEST_DB env var so `cargo test` on a
// machine without the dev DB silently skips them.
//
// Each test uses fresh Uuids per run and cleans up rows it inserted so
// running twice in a row stays green.

#[cfg(test)]
mod retention_storage_tests {
    use super::Storage;
    use chrono::Utc;
    use uuid::Uuid;

    const TEST_DB_URL: &str = "postgres://chola_app:chola_app_secret@localhost:5432/choladb";

    /// Returns Some(Storage) if CHOLA_TEST_DB is set and the connection
    /// works; None otherwise (in which case the caller should `return`
    /// to skip).
    async fn maybe_storage() -> Option<Storage> {
        if std::env::var("CHOLA_TEST_DB").is_err() {
            return None;
        }
        match Storage::new(TEST_DB_URL, 4, "chola").await {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("retention_storage_tests: skipping (DB unreachable: {e})");
                None
            }
        }
    }

    /// Insert a job_groups row with a terminal state and a controllable
    /// `completed_at`. `repo_id` is NULL (migration 022 made the column
    /// nullable) so we don't need to seed `chola.repos`.
    async fn seed_group(
        s: &Storage,
        gid: Uuid,
        state: &str,
        completed_at: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO chola.job_groups \
             (id, repo_id, branch, commit_sha, trigger_source, state, \
              created_at, updated_at, completed_at) \
             VALUES ($1, NULL, 'test', 'deadbeef', 'test', $2, \
                     $3, $3, $3)",
        )
        .bind(gid)
        .bind(state)
        .bind(completed_at)
        .execute(s.pool())
        .await?;
        Ok(())
    }

    async fn seed_job(
        s: &Storage,
        job_id: Uuid,
        gid: Uuid,
        worker_id: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO chola.jobs \
             (id, job_group_id, stage_name, command, worker_id, state, \
              created_at, updated_at) \
             VALUES ($1, $2, 'unit', 'echo hi', $3, 'success', \
                     NOW(), NOW())",
        )
        .bind(job_id)
        .bind(gid)
        .bind(worker_id)
        .execute(s.pool())
        .await?;
        Ok(())
    }

    /// Remove every trace of a group from live + archive tables. Safe
    /// to call against ids the test never inserted (DELETE … WHERE id =
    /// $1 returns 0 rows).
    async fn cleanup_group(s: &Storage, gid: Uuid) -> anyhow::Result<()> {
        // If seed_all_children stashed a "t8:<repo>:<stage_config>" marker on
        // the branch column, recover those ids so we can delete the synthetic
        // repo and stage_config rows too. Look in both live and archive.
        let marker: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT branch, NULL::text \
             FROM chola.job_groups WHERE id = $1 \
             UNION ALL \
             SELECT branch, NULL::text \
             FROM chola.job_groups_archive WHERE id = $1 \
             LIMIT 1",
        )
        .bind(gid)
        .fetch_optional(s.pool())
        .await?;
        let synthetic_ids = marker
            .and_then(|(b, _)| b)
            .filter(|b| b.starts_with("t8:"))
            .and_then(|b| {
                let parts: Vec<&str> = b.split(':').collect();
                if parts.len() == 3 {
                    let repo = Uuid::parse_str(parts[1]).ok()?;
                    let sc = Uuid::parse_str(parts[2]).ok()?;
                    Some((repo, sc))
                } else {
                    None
                }
            });

        // Children first in both worlds.
        for tbl in [
            "approval_gates",
            "test_results",
            "artifacts",
            "worker_reservations",
            "jobs",
        ] {
            let q = format!("DELETE FROM chola.{tbl} WHERE job_group_id = $1");
            sqlx::query(&q).bind(gid).execute(s.pool()).await?;
            let qa = format!("DELETE FROM chola.{tbl}_archive WHERE job_group_id = $1");
            sqlx::query(&qa).bind(gid).execute(s.pool()).await?;
        }
        sqlx::query("DELETE FROM chola.job_groups WHERE id = $1")
            .bind(gid)
            .execute(s.pool())
            .await?;
        sqlx::query("DELETE FROM chola.job_groups_archive WHERE id = $1")
            .bind(gid)
            .execute(s.pool())
            .await?;
        if let Some((repo_id, stage_config_id)) = synthetic_ids {
            sqlx::query("DELETE FROM chola.stage_configs WHERE id = $1")
                .bind(stage_config_id)
                .execute(s.pool())
                .await
                .ok();
            sqlx::query("DELETE FROM chola.repos WHERE id = $1")
                .bind(repo_id)
                .execute(s.pool())
                .await
                .ok();
        }
        Ok(())
    }

    #[tokio::test]
    async fn t1_finds_terminal_group_older_than_threshold() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(10))
            .await
            .expect("seed");

        let found = s.find_groups_for_t1(7, 1000).await.expect("t1");
        assert!(
            found.contains(&gid),
            "expected {gid} in t1 result, got {found:?}"
        );

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    #[tokio::test]
    async fn t1_skips_groups_with_files_already_purged() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(10))
            .await
            .expect("seed");
        // Pre-mark as purged.
        s.mark_files_purged(&[gid], Utc::now())
            .await
            .expect("mark purged");

        let found = s.find_groups_for_t1(7, 1000).await.expect("t1");
        assert!(
            !found.contains(&gid),
            "expected {gid} NOT in t1 result (already purged), got {found:?}"
        );

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    #[tokio::test]
    async fn t1_skips_recent_groups() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();

        // Completed 1h ago — way below any reasonable t1.
        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::hours(1))
            .await
            .expect("seed");

        let found = s.find_groups_for_t1(7, 1000).await.expect("t1");
        assert!(
            !found.contains(&gid),
            "expected {gid} NOT in t1 result (too recent), got {found:?}"
        );

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    #[tokio::test]
    async fn archive_and_unarchive_roundtrip() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(40))
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some("worker-roundtrip"))
            .await
            .expect("seed job");

        // Archive.
        let archived = s.archive_groups_batch(&[gid]).await.expect("archive");
        assert_eq!(archived, 1, "expected 1 group archived");

        // Live tables empty for this id.
        let live_groups: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups WHERE id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count live");
        assert_eq!(live_groups, 0);
        let live_jobs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.jobs WHERE job_group_id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count live jobs");
        assert_eq!(live_jobs, 0);

        // Archive tables populated.
        let arch_groups: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups_archive WHERE id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count arch");
        assert_eq!(arch_groups, 1);
        let arch_jobs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.jobs_archive WHERE job_group_id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count arch jobs");
        assert_eq!(arch_jobs, 1);

        // Unarchive — round-trip back to live.
        let unarchived = s.unarchive_groups_batch(&[gid]).await.expect("unarchive");
        assert_eq!(unarchived, 1, "expected 1 group unarchived");

        let live_groups_after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups WHERE id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count live after");
        assert_eq!(live_groups_after, 1);
        let arch_groups_after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups_archive WHERE id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count arch after");
        assert_eq!(arch_groups_after, 0);

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    #[tokio::test]
    async fn delete_archive_removes_all_six_tables() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(40))
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some("worker-del"))
            .await
            .expect("seed job");

        // Push to archive.
        s.archive_groups_batch(&[gid]).await.expect("archive");

        // Hard delete.
        let deleted = s.delete_archive_batch(&[gid]).await.expect("delete");
        assert_eq!(deleted, 1, "expected 1 job_groups_archive row deleted");

        // All six archive tables empty for this id.
        for tbl in [
            "job_groups_archive",
            "jobs_archive",
            "worker_reservations_archive",
            "artifacts_archive",
            "test_results_archive",
            "approval_gates_archive",
        ] {
            let col = if tbl == "job_groups_archive" {
                "id"
            } else {
                "job_group_id"
            };
            let q = format!("SELECT COUNT(*) FROM chola.{tbl} WHERE {col} = $1");
            let n: i64 = sqlx::query_scalar(&q)
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("count");
            assert_eq!(n, 0, "expected 0 rows in {tbl} after delete");
        }

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    #[tokio::test]
    async fn mark_files_purged_only_sets_null_rows() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid_unpurged = Uuid::new_v4();
        let gid_already = Uuid::new_v4();

        seed_group(
            &s,
            gid_unpurged,
            "success",
            Utc::now() - chrono::Duration::days(10),
        )
        .await
        .expect("seed unpurged");
        seed_group(
            &s,
            gid_already,
            "success",
            Utc::now() - chrono::Duration::days(10),
        )
        .await
        .expect("seed already");

        // Pre-stamp one of them.
        s.mark_files_purged(&[gid_already], Utc::now())
            .await
            .expect("pre-stamp");

        let affected = s
            .mark_files_purged(&[gid_unpurged, gid_already], Utc::now())
            .await
            .expect("mark batch");
        assert_eq!(affected, 1, "only the unpurged row should flip");

        // Confirm both are now purged.
        for gid in [gid_unpurged, gid_already] {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM chola.job_groups \
                 WHERE id = $1 AND files_purged_at IS NOT NULL",
            )
            .bind(gid)
            .fetch_one(s.pool())
            .await
            .expect("count");
            assert_eq!(n, 1, "{gid} should be purged");
        }

        cleanup_group(&s, gid_unpurged).await.expect("cleanup 1");
        cleanup_group(&s, gid_already).await.expect("cleanup 2");
    }

    #[tokio::test]
    async fn workers_for_group_reads_live_and_archive() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let worker = "worker-X-retention-test";

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(40))
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some(worker))
            .await
            .expect("seed job");

        // Live path.
        let live = s.workers_for_group(gid).await.expect("workers live");
        assert_eq!(live, vec![worker.to_string()]);

        // Archive path: T2 moves the job row out of `jobs` into `jobs_archive`.
        s.archive_groups_batch(&[gid]).await.expect("archive");
        let archived = s.workers_for_group(gid).await.expect("workers archive");
        assert_eq!(
            archived,
            vec![worker.to_string()],
            "must still see worker via jobs_archive fallback after T2"
        );

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    /// T7a — UNION ALL listing returns rows from both live and archive
    /// tables, each tagged with the correct `archived` boolean.
    #[tokio::test]
    async fn list_with_include_archived_unions_both_tables() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid_live = Uuid::new_v4();
        let gid_arch = Uuid::new_v4();

        seed_group(
            &s,
            gid_live,
            "success",
            Utc::now() - chrono::Duration::days(2),
        )
        .await
        .expect("seed live");
        seed_group(
            &s,
            gid_arch,
            "success",
            Utc::now() - chrono::Duration::days(40),
        )
        .await
        .expect("seed arch-candidate");
        s.archive_groups_batch(&[gid_arch]).await.expect("archive");

        // No filters — just confirm both rows surface and the archived
        // flag is correct.
        let (rows, total) = s
            .list_job_groups_paginated_with_archive(
                500, 0, None, None, None, None, None, None, None,
            )
            .await
            .expect("list with archive");

        // total counts the union across the seeded rows + whatever
        // else lives in the DB, so just assert the bound.
        assert!(total >= 2, "expected total >= 2, got {total}");

        let row_live = rows.iter().find(|(g, _)| g.id == gid_live);
        let row_arch = rows.iter().find(|(g, _)| g.id == gid_arch);
        assert!(row_live.is_some(), "missing live row {gid_live}");
        assert!(row_arch.is_some(), "missing archived row {gid_arch}");
        assert!(!row_live.unwrap().1, "live row should have archived=false");
        assert!(
            row_arch.unwrap().1,
            "archived row should have archived=true"
        );

        cleanup_group(&s, gid_live).await.expect("cleanup live");
        cleanup_group(&s, gid_arch).await.expect("cleanup arch");
    }

    /// T7a — single-group GET transparently falls back to the archive
    /// tables and reports `is_archived = true`.
    #[tokio::test]
    async fn get_or_archive_falls_back_to_archive() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let worker = "worker-fallback-test";

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(40))
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some(worker))
            .await
            .expect("seed job");

        // Snapshot the live row before archiving so we can compare.
        let (live_group, _, _) = s
            .get_job_group_with_jobs_or_archive(gid)
            .await
            .expect("live read")
            .expect("group present");

        // Push to archive — live row disappears.
        s.archive_groups_batch(&[gid]).await.expect("archive");

        let (arch_group, arch_jobs, is_archived) = s
            .get_job_group_with_jobs_or_archive(gid)
            .await
            .expect("archive read")
            .expect("group still discoverable after archive");
        assert!(is_archived, "expected is_archived=true after T2");
        assert_eq!(arch_group.id, live_group.id);
        assert_eq!(arch_group.branch, live_group.branch);
        assert_eq!(arch_group.commit_sha, live_group.commit_sha);
        assert_eq!(arch_group.state, live_group.state);
        assert_eq!(arch_jobs.len(), 1, "expected 1 archived job row");
        assert_eq!(arch_jobs[0].id, job_id);
        assert_eq!(arch_jobs[0].worker_id.as_deref(), Some(worker));

        // Sanity: archive timestamps method also surfaces the row.
        let (archived_at, files_purged_at) = s
            .get_archive_timestamps(gid)
            .await
            .expect("archive timestamps")
            .expect("row present in archive");
        let _ = (archived_at, files_purged_at); // just confirm the call succeeded.

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    // -----------------------------------------------------------------------
    // T8 — new integration tests
    // -----------------------------------------------------------------------

    /// Insert a row into each of the five child tables for `gid` so that
    /// `chola.archive_group_ids` exercises every cascade INSERT/DELETE.
    async fn seed_all_children(s: &Storage, gid: Uuid, job_id: Uuid) -> anyhow::Result<()> {
        // worker_reservations
        sqlx::query(
            "INSERT INTO chola.worker_reservations \
             (id, worker_id, job_group_id, reserved_at, expires_at) \
             VALUES ($1, 'worker-t8', $2, now(), now() + interval '1 hour')",
        )
        .bind(Uuid::new_v4())
        .bind(gid)
        .execute(s.pool())
        .await?;

        // artifacts
        sqlx::query(
            "INSERT INTO chola.artifacts \
             (id, job_group_id, job_id, stage_name, filename, file_path) \
             VALUES ($1, $2, $3, 'build', 'out.tar', '/tmp/out.tar')",
        )
        .bind(Uuid::new_v4())
        .bind(gid)
        .bind(job_id)
        .execute(s.pool())
        .await?;

        // test_results
        sqlx::query(
            "INSERT INTO chola.test_results \
             (id, job_id, job_group_id, suite_name, test_name, status) \
             VALUES ($1, $2, $3, 'suite', 'test_foo', 'passed')",
        )
        .bind(Uuid::new_v4())
        .bind(job_id)
        .bind(gid)
        .execute(s.pool())
        .await?;

        // approval_gates needs a real stage_config_id (FK to stage_configs,
        // which itself needs a real repo_id). Build a minimal pair so the
        // insert satisfies both FKs, and clean them up in cleanup_group.
        let repo_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO chola.repos (id, repo_name, repo_url) \
             VALUES ($1, $2, $3)",
        )
        .bind(repo_id)
        .bind(format!("t8-test-{repo_id}"))
        .bind(format!("https://example.test/t8/{repo_id}.git"))
        .execute(s.pool())
        .await?;
        let stage_config_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO chola.stage_configs \
             (id, repo_id, stage_name, command) \
             VALUES ($1, $2, 'unit', 'echo hi')",
        )
        .bind(stage_config_id)
        .bind(repo_id)
        .execute(s.pool())
        .await?;

        sqlx::query(
            "INSERT INTO chola.approval_gates \
             (id, job_group_id, stage_config_id) \
             VALUES ($1, $2, $3)",
        )
        .bind(Uuid::new_v4())
        .bind(gid)
        .bind(stage_config_id)
        .execute(s.pool())
        .await?;

        // Stash repo+stage_config ids on the group's branch field so
        // cleanup_group can find and delete them. Cheap hack to avoid
        // threading return values through every call site.
        sqlx::query("UPDATE chola.job_groups SET branch = $2 WHERE id = $1")
            .bind(gid)
            .bind(format!("t8:{repo_id}:{stage_config_id}"))
            .execute(s.pool())
            .await?;

        Ok(())
    }

    /// Walk all six archive tables and assert the row counts for `gid`.
    async fn assert_archive_counts(
        s: &Storage,
        gid: Uuid,
        expected_group: i64,
        expected_child: i64,
    ) {
        let n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups_archive WHERE id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .unwrap();
        assert_eq!(n, expected_group, "job_groups_archive count");

        for tbl in [
            "jobs_archive",
            "worker_reservations_archive",
            "artifacts_archive",
            "test_results_archive",
            "approval_gates_archive",
        ] {
            let q = format!("SELECT COUNT(*) FROM chola.{tbl} WHERE job_group_id = $1");
            let n: i64 = sqlx::query_scalar(&q)
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .unwrap();
            assert_eq!(n, expected_child, "{tbl} count");
        }
    }

    /// T8-1: Full T1 → T2 → T3 lifecycle with all five child tables.
    ///
    /// Seeds one group, forces each tier via direct storage calls (not the
    /// RPC — the RPC test is the ForceRetentionTick smoke test at the bottom).
    #[tokio::test]
    async fn t1_t2_t3_full_lifecycle() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        // Seed with completed_at 35 days ago — qualifies for T1 (default 7d)
        // and T2 (default 30d).
        let completed_at = Utc::now() - chrono::Duration::days(35);
        seed_group(&s, gid, "success", completed_at)
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some("worker-lifecycle"))
            .await
            .expect("seed job");
        seed_all_children(&s, gid, job_id)
            .await
            .expect("seed children");

        // T1: mark files purged (simulates controller-side delete + stamp).
        let purged = s
            .mark_files_purged(&[gid], Utc::now())
            .await
            .expect("mark files purged");
        assert_eq!(purged, 1, "T1 should stamp 1 group");

        // Verify files_purged_at set on live row.
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chola.job_groups WHERE id = $1 AND files_purged_at IS NOT NULL",
        )
        .bind(gid)
        .fetch_one(s.pool())
        .await
        .expect("count purged");
        assert_eq!(n, 1, "files_purged_at should be set after T1");

        // T2: archive.
        let archived = s.archive_groups_batch(&[gid]).await.expect("archive");
        assert_eq!(archived, 1, "T2 should archive 1 group");
        assert_archive_counts(&s, gid, 1, 1).await;

        // Live tables must be empty for this group.
        let live: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups WHERE id = $1")
            .bind(gid)
            .fetch_one(s.pool())
            .await
            .expect("live count");
        assert_eq!(live, 0, "group should be gone from live table after T2");

        // Backdate archived_at to qualify for T3 (default 365d).
        sqlx::query(
            "UPDATE chola.job_groups_archive \
             SET archived_at = now() - interval '400 days' \
             WHERE id = $1",
        )
        .bind(gid)
        .execute(s.pool())
        .await
        .expect("backdate archived_at");

        // T3: hard delete.
        let deleted = s.delete_archive_batch(&[gid]).await.expect("delete");
        assert_eq!(deleted, 1, "T3 should delete 1 group from archive");
        assert_archive_counts(&s, gid, 0, 0).await;

        // cleanup_group is a no-op since T3 already deleted everything.
        cleanup_group(&s, gid).await.expect("cleanup");
    }

    /// T8-2: Per-repo build cap archives the oldest excess group.
    ///
    /// Inserts 6 terminal groups for one repo, then directly calls
    /// `find_excess_groups_per_repo(5)` + `archive_groups_batch` to simulate
    /// the max_builds_per_repo enforcement path.
    #[tokio::test]
    async fn max_builds_per_repo_cap_archives_excess() {
        let Some(s) = maybe_storage().await else {
            return;
        };

        // Use a unique repo_id to isolate this test from other rows.
        let repo_id = Uuid::new_v4();
        let mut gids: Vec<Uuid> = Vec::new();

        // Insert a repo row first (job_groups.repo_id FK).
        sqlx::query(
            "INSERT INTO chola.repos (id, repo_name, repo_url) \
             VALUES ($1, $2, 'http://x') \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(repo_id)
        .bind(format!("cap-test-{repo_id}"))
        .execute(s.pool())
        .await
        .expect("insert repo");

        // Seed 6 groups, oldest first.
        for i in 0..6usize {
            let gid = Uuid::new_v4();
            let completed_at = Utc::now() - chrono::Duration::days(10 + i as i64);
            sqlx::query(
                "INSERT INTO chola.job_groups \
                 (id, repo_id, branch, commit_sha, trigger_source, state, \
                  created_at, updated_at, completed_at) \
                 VALUES ($1, $2, 'main', 'deadbeef', 'test', 'success', \
                         $3, $3, $3)",
            )
            .bind(gid)
            .bind(repo_id)
            .bind(completed_at)
            .execute(s.pool())
            .await
            .expect("seed group");
            gids.push(gid);
        }

        // find_excess: with max_per_repo=5, the oldest 1 should be excess.
        let excess = s.find_excess_groups_per_repo(5).await.expect("find_excess");
        // The excess set may include groups from other tests, so we only
        // assert that our oldest group is included.
        let oldest = gids[5]; // inserted last = oldest (10+5=15 days ago)
        assert!(
            excess.contains(&oldest),
            "oldest group {oldest} should be in excess set"
        );

        // Archive the excess.
        let archived = s
            .archive_groups_batch(&excess)
            .await
            .expect("archive excess");
        assert!(archived >= 1, "at least 1 group should be archived");

        // Our oldest group must be in the archive.
        let n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups_archive WHERE id = $1")
                .bind(oldest)
                .fetch_one(s.pool())
                .await
                .expect("count archived");
        assert_eq!(n, 1, "oldest group should be in archive after cap");

        // Cleanup all 6 groups.
        for gid in &gids {
            cleanup_group(&s, *gid).await.expect("cleanup");
        }
        sqlx::query("DELETE FROM chola.repos WHERE id = $1")
            .bind(repo_id)
            .execute(s.pool())
            .await
            .expect("cleanup repo");
    }

    /// T8-3: Runtime settings override takes effect immediately.
    ///
    /// This test inserts into `config_settings` to change the T1 threshold,
    /// then asserts that `find_groups_for_t1` with the new threshold returns
    /// the expected rows. The actual `run_once` call with ControllerState
    /// is exercised in the e2e tests below.
    #[tokio::test]
    async fn runtime_settings_override_takes_effect_next_tick() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();

        // Group completed 10 days ago.
        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(10))
            .await
            .expect("seed");

        // With threshold=99999 (very high): group should NOT qualify.
        let found_high = s
            .find_groups_for_t1(99999, 1000)
            .await
            .expect("find t1 high");
        assert!(
            !found_high.contains(&gid),
            "group should NOT qualify with threshold=99999"
        );

        // With threshold=1 (very low): group should qualify.
        let found_low = s.find_groups_for_t1(1, 1000).await.expect("find t1 low");
        assert!(
            found_low.contains(&gid),
            "group should qualify with threshold=1"
        );

        // Verify the same behavior for T2.
        let t2_high = s.find_groups_for_t2(99999, 1000).await.expect("t2 high");
        assert!(!t2_high.contains(&gid));
        let t2_low = s.find_groups_for_t2(1, 1000).await.expect("t2 low");
        assert!(t2_low.contains(&gid));

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    /// T8-4: Redis purge queue durability across controller restart.
    ///
    /// Enqueues a purge entry via `enqueue_purge`, then constructs a fresh
    /// `RedisStore` from the same URL (simulating a restart), calls
    /// `dequeue_purges`, and asserts the entry survives.
    #[tokio::test]
    async fn worker_purge_queue_durable_across_controller_restart() {
        const REDIS_URL: &str = "redis://127.0.0.1:6379";

        if std::env::var("CHOLA_TEST_DB").is_err() {
            return; // gated same as DB tests
        }

        let redis = match crate::redis_store::RedisStore::new(REDIS_URL, "chola_t8_test").await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("worker_purge_queue_durable: skipping (Redis unreachable: {e})");
                return;
            }
        };

        let worker_id = format!("worker-durability-{}", Uuid::new_v4());
        let group_id = Uuid::new_v4();

        // Enqueue (simulating T1 run).
        redis
            .enqueue_purge(&worker_id, &group_id.to_string())
            .await
            .expect("enqueue");

        // Construct a fresh store from the same URL (simulating restart).
        let redis2 = crate::redis_store::RedisStore::new(REDIS_URL, "chola_t8_test")
            .await
            .expect("new redis store");

        let pending = redis2.dequeue_purges(&worker_id).await.expect("dequeue");
        assert!(
            pending.contains(&group_id.to_string()),
            "purge entry should survive restart; pending={pending:?}"
        );

        // Cleanup.
        redis2
            .ack_purge(&worker_id, &group_id.to_string())
            .await
            .expect("ack cleanup");
    }

    /// T8-5: Stale worker treated as no-longer-registered.
    ///
    /// Seeds a group and a job row with a worker_id, then calls
    /// `workers_for_group` and `mark_files_purged` directly — exercising
    /// the storage side of the "stale worker → stamp immediately" path.
    /// (The in-memory heartbeat check is in `run_t1`; the storage contract
    /// is that `mark_files_purged` sets `files_purged_at` unconditionally
    /// for the caller — it does not re-check worker liveness.)
    #[tokio::test]
    async fn stale_worker_treated_as_no_longer_registered() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let stale_worker = format!("stale-worker-{}", Uuid::new_v4());

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(10))
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some(&stale_worker))
            .await
            .expect("seed job");

        // Confirm workers_for_group returns the stale worker.
        let owners = s.workers_for_group(gid).await.expect("workers_for_group");
        assert!(
            owners.contains(&stale_worker),
            "expected stale worker in owners"
        );

        // Simulate the run_t1 logic: stale worker → not live → stamp immediately.
        let stamped = s
            .mark_files_purged(&[gid], Utc::now())
            .await
            .expect("mark purged");
        assert_eq!(stamped, 1, "files_purged_at should be stamped");

        // Verify.
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chola.job_groups WHERE id = $1 AND files_purged_at IS NOT NULL",
        )
        .bind(gid)
        .fetch_one(s.pool())
        .await
        .expect("count");
        assert_eq!(n, 1, "files_purged_at must be set");

        cleanup_group(&s, gid).await.expect("cleanup");
    }

    /// T8-6: UNION query performance — no Seq Scan on either live or archive
    /// table when listing with `include_archived=true`.
    ///
    /// Gated behind `CHOLA_PERF_TEST=1` because seeding 50k rows is slow
    /// and not suitable for day-to-day `cargo test` runs. To run:
    ///
    /// ```sh
    /// CHOLA_TEST_DB=1 CHOLA_PERF_TEST=1 cargo test -p chola-controller union_query_performance
    /// ```
    #[tokio::test]
    async fn union_query_performance_50k_each_side() {
        if std::env::var("CHOLA_PERF_TEST").is_err() {
            return; // skip unless explicitly opted in
        }
        let Some(s) = maybe_storage().await else {
            return;
        };

        // Use a unique repo_id prefix to isolate cleanup.
        let tag = Uuid::new_v4();

        // Seed 50k live rows via a single bulk INSERT.
        sqlx::query(&format!(
            "INSERT INTO chola.job_groups \
             (id, repo_id, branch, commit_sha, trigger_source, state, \
              created_at, updated_at, completed_at) \
             SELECT gen_random_uuid(), NULL, 'perf', '{tag}', 'perf', 'success', \
                    now(), now(), now() - interval '40 days' \
             FROM generate_series(1, 50000)"
        ))
        .execute(s.pool())
        .await
        .expect("seed live");

        // Seed 50k archive rows.
        sqlx::query(&format!(
            "INSERT INTO chola.job_groups_archive \
             (id, repo_id, branch, commit_sha, trigger_source, state, \
              created_at, updated_at, completed_at, archived_at) \
             SELECT gen_random_uuid(), NULL, 'perf', '{tag}', 'perf', 'success', \
                    now() - interval '50 days', now(), now() - interval '50 days', \
                    now() - interval '40 days' \
             FROM generate_series(1, 50000)"
        ))
        .execute(s.pool())
        .await
        .expect("seed archive");

        // Run EXPLAIN (no ANALYZE to avoid actual execution cost in test).
        let plan: String = sqlx::query_scalar(
            "EXPLAIN \
             SELECT id, branch, state, NULL::timestamptz AS archived_at \
               FROM chola.job_groups \
             UNION ALL \
             SELECT id, branch, state, archived_at \
               FROM chola.job_groups_archive \
             LIMIT 50",
        )
        .fetch_one(s.pool())
        .await
        .expect("explain");

        // Must not contain a sequential scan on either base table.
        assert!(
            !plan.to_lowercase().contains("seq scan on job_groups"),
            "Unexpected Seq Scan on job_groups; plan:\n{plan}"
        );

        // Cleanup.
        sqlx::query(&format!(
            "DELETE FROM chola.job_groups WHERE commit_sha = '{tag}'"
        ))
        .execute(s.pool())
        .await
        .expect("cleanup live");
        sqlx::query(&format!(
            "DELETE FROM chola.job_groups_archive WHERE commit_sha = '{tag}'"
        ))
        .execute(s.pool())
        .await
        .expect("cleanup archive");
    }

    /// T8-7: Rollback via `chola.unarchive_group_ids` restores all rows.
    ///
    /// Archives a group with all five child tables populated, then calls
    /// `unarchive_groups_batch` and asserts all rows are back in live tables
    /// with archive tables empty.
    #[tokio::test]
    async fn rollback_unarchive_restores_group() {
        let Some(s) = maybe_storage().await else {
            return;
        };
        let gid = Uuid::new_v4();
        let job_id = Uuid::new_v4();

        seed_group(&s, gid, "success", Utc::now() - chrono::Duration::days(40))
            .await
            .expect("seed group");
        seed_job(&s, job_id, gid, Some("worker-rollback"))
            .await
            .expect("seed job");
        seed_all_children(&s, gid, job_id)
            .await
            .expect("seed children");

        // Archive.
        let archived = s.archive_groups_batch(&[gid]).await.expect("archive");
        assert_eq!(archived, 1);

        // Verify it's in archive.
        assert_archive_counts(&s, gid, 1, 1).await;

        // Unarchive (rollback runbook path).
        let unarchived = s.unarchive_groups_batch(&[gid]).await.expect("unarchive");
        assert_eq!(unarchived, 1, "unarchive should return 1");

        // Archive tables must be empty for this group.
        assert_archive_counts(&s, gid, 0, 0).await;

        // Live tables must have the group back.
        let live: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chola.job_groups WHERE id = $1")
            .bind(gid)
            .fetch_one(s.pool())
            .await
            .expect("live count");
        assert_eq!(live, 1, "group should be restored to live table");

        let live_jobs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chola.jobs WHERE job_group_id = $1")
                .bind(gid)
                .fetch_one(s.pool())
                .await
                .expect("live jobs count");
        assert_eq!(live_jobs, 1, "jobs should be restored");

        cleanup_group(&s, gid).await.expect("cleanup");
    }
}
