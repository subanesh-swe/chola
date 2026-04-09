use std::collections::HashMap;
use std::sync::Arc;

use clap::Parser;
use tokio::sync::{Notify, RwLock};
use tracing::{error, info, warn};

use ci_core::models::job::{Job, JobState, JobType};
use ci_core::models::worker::{DiskType, WorkerInfo, WorkerState, WorkerStatus};

mod api;
mod auth;
mod csrf;
mod dag;
mod grpc_server;
mod http_server;
mod job_group_registry;
mod job_registry;
mod log_aggregator;
mod monitoring;
mod notifier;
pub mod openapi;
mod rate_limit;
mod redis_store;
mod reservation;
mod retention;
mod scheduler;
mod state;
mod storage;
mod worker_registry;

use auth::middleware::AuthConfig;
use ci_core::models::config::ControllerConfig;
use job_group_registry::JobGroupRegistry;
use job_registry::JobRegistry;
use log_aggregator::LogAggregator;
use monitoring::Metrics;
use state::ControllerState;
use worker_registry::WorkerRegistry;

/// CI Orchestrator - Controller
#[derive(Parser, Debug)]
#[command(name = "ci-controller", about = "CI Orchestrator Controller")]
struct Cli {
    /// Path to controller YAML config file
    /// Defaults: ~/.config/chola/controller.yaml → /etc/chola/controller.yaml
    #[arg(short, long)]
    config: Option<String>,

    /// Override bind address
    #[arg(long)]
    bind: Option<String>,

    /// Override log level (trace, debug, info, warn, error)
    #[arg(long)]
    log_level: Option<String>,

    /// Override HTTP sidecar port (from config: http_port)
    #[arg(long)]
    http_port: Option<u16>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config_path = cli.config
        .or_else(|| ci_core::models::config::resolve_default_config("controller"))
        .ok_or_else(|| anyhow::anyhow!(
            "No config file found. Pass --config or create ~/.config/chola/controller.yaml or /etc/chola/controller.yaml"
        ))?;
    let mut config = ControllerConfig::from_file(&config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load config '{}': {}", config_path, e))?;

    // Apply CLI overrides
    if let Some(bind) = cli.bind {
        config.bind_address = bind;
    }
    if let Some(port) = cli.http_port {
        config.http_port = port;
    }

    let log_level = cli.log_level.as_deref().unwrap_or(&config.logging.level);

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    // Warn if default JWT secret is in use
    if config.auth.enabled && config.auth.jwt_secret == "change-me-in-production" {
        warn!("Using default JWT secret! Set auth.jwt_secret in config for production.");
    }

    // Warn if auth is enabled but HTTP traffic is unencrypted
    if config.auth.enabled && config.http_tls.is_none() {
        warn!("Auth enabled but HTTP TLS not configured. API traffic is unencrypted.");
        warn!("Set http_tls in config for production, or use a reverse proxy (nginx) with TLS.");
    }

    info!("Starting CI Controller");
    info!("Bind address: {}", config.bind_address);
    info!("HTTP port: {}", config.http_port);
    info!("Scheduling strategy: {}", config.scheduling.strategy);

    // Connect to PostgreSQL (non-fatal)
    let (db_url, db_sources) = config.storage.postgres.database_url();
    for (field, source) in &db_sources {
        info!("pg {}: from {}", field, source);
    }
    let storage = match storage::Storage::new(
        &db_url,
        config.storage.postgres.max_connections,
        &config.storage.postgres.schema,
    )
    .await
    {
        Ok(s) => {
            info!("Connected to PostgreSQL");
            let s = s.with_encryption_key(config.auth.encryption_key.clone());
            if let Err(e) = s.migrate().await {
                warn!("Migration failed: {}", e);
            }
            Some(Arc::new(s))
        }
        Err(e) => {
            warn!("PostgreSQL unavailable: {}", e);
            None
        }
    };

    // Seed default admin user if configured and storage available
    if let (Some(storage), Some(username), Some(password)) = (
        &storage,
        &config.auth.default_admin_username,
        &config.auth.default_admin_password,
    ) {
        match storage.get_user_by_username(username).await {
            Ok(None) => match crate::auth::password::hash_password(password) {
                Ok(hash) => {
                    match storage
                        .create_user(username, &hash, Some("Default Admin"), "super_admin")
                        .await
                    {
                        Ok(_) => info!("Default admin user '{}' created", username),
                        Err(e) => warn!("Failed to create default admin: {}", e),
                    }
                }
                Err(e) => warn!("Failed to hash admin password: {}", e),
            },
            Ok(Some(_)) => info!("Default admin user '{}' already exists", username),
            Err(e) => warn!("Failed to check admin user: {}", e),
        }
    }

    // Connect to Redis (non-fatal)
    let (redis_url, redis_sources) = config.redis.redis_url();
    for (field, source) in &redis_sources {
        info!("redis {}: from {}", field, source);
    }
    let redis_store = match redis_store::RedisStore::new(&redis_url, &config.redis.key_prefix).await
    {
        Ok(r) => {
            info!("Connected to Redis");
            Some(Arc::new(r))
        }
        Err(e) => {
            warn!("Redis unavailable: {}", e);
            None
        }
    };

    // Build log aggregator — always use disk so logs survive restarts
    let log_dir = config
        .logging
        .log_dir
        .clone()
        .unwrap_or_else(|| ci_core::models::config::chola_data_dir("controller/logs"));
    tokio::fs::create_dir_all(&log_dir).await?;
    let log_agg = LogAggregator::with_log_dir(log_dir.clone());
    info!("Log directory: {}", log_agg.log_dir().unwrap_or("none"));

    // Build auth config for middleware
    let mut auth_config = AuthConfig::from_controller_config(&config.auth);
    auth_config.storage = storage.clone();

    // Load all active token hashes for gRPC interceptor cache
    let mut token_hashes: std::collections::HashSet<String> = if let Some(s) = &storage {
        match s.list_worker_tokens().await {
            Ok(tokens) => tokens
                .into_iter()
                .filter(|t| t.active)
                .map(|t| t.token_hash)
                .collect(),
            Err(e) => {
                warn!("Failed to load token hashes: {}", e);
                std::collections::HashSet::new()
            }
        }
    } else {
        std::collections::HashSet::new()
    };
    // Also load worker token hashes (chola_wkr_ tokens from registration)
    if let Some(s) = &storage {
        match s.load_workers().await {
            Ok(workers) => {
                let count_before = token_hashes.len();
                for w in workers {
                    if let Some(h) = w.worker_token_hash {
                        token_hashes.insert(h);
                    }
                }
                let added = token_hashes.len() - count_before;
                if added > 0 {
                    info!("Added {} worker token hash(es) to cache", added);
                }
            }
            Err(e) => warn!("Failed to load worker token hashes: {}", e),
        }
    }
    info!("Loaded {} token hash(es) into cache", token_hashes.len());

    // Construct the shared ControllerState
    let state = Arc::new(ControllerState {
        config: config.clone(),
        auth_config,
        worker_registry: Arc::new(RwLock::new(WorkerRegistry::new())),
        job_registry: RwLock::new(JobRegistry::new()),
        log_aggregator: RwLock::new(log_agg),
        job_group_registry: Arc::new(RwLock::new(JobGroupRegistry::new())),
        job_stream_senders: RwLock::new(HashMap::new()),
        scheduler_notify: Notify::new(),
        metrics: Metrics::new(),
        storage,
        redis_store,
        log_dir,
        token_hashes: Arc::new(std::sync::RwLock::new(token_hashes)),
    });

    // ── State recovery ────────────────────────────────────────────────────────
    if let Some(storage) = &state.storage {
        // Recover job groups
        match storage.load_active_job_groups().await {
            Ok(groups) => {
                let mut jgr = state.job_group_registry.write().await;
                for group in &groups {
                    jgr.add_group(group.clone());
                }
                if !groups.is_empty() {
                    info!("Recovered {} active job groups from DB", groups.len());
                }
            }
            Err(e) => warn!("Failed to recover job groups: {}", e),
        }

        // Expire stale reservations recovered from DB.
        // Groups stuck in Reserved for longer than reservation_timeout_secs
        // are dead weight — the worker never submitted stages.
        {
            let timeout =
                std::time::Duration::from_secs(state.config.workers.reservation_timeout_secs);
            let chrono_timeout =
                chrono::Duration::from_std(timeout).unwrap_or(chrono::Duration::hours(4));
            let now = chrono::Utc::now();

            let stale_ids: Vec<(uuid::Uuid, Option<String>)>;
            {
                let mut jgr = state.job_group_registry.write().await;
                stale_ids = jgr
                    .active_groups()
                    .iter()
                    .filter(|g| {
                        g.state == ci_core::models::job_group::JobGroupState::Reserved
                            && (now - g.created_at) > chrono_timeout
                    })
                    .map(|g| (g.id, g.reserved_worker_id.clone()))
                    .collect();

                for (gid, _) in &stale_ids {
                    jgr.update_state(gid, ci_core::models::job_group::JobGroupState::Expired);
                    if let Some(g) = jgr.get_mut(gid) {
                        g.status_reason = Some(
                            "Reservation expired on startup (stale from previous run)".to_string(),
                        );
                    }
                }
            }

            if !stale_ids.is_empty() {
                let startup_reason = "Reservation expired on startup (stale from previous run)";
                for (gid, worker_id) in &stale_ids {
                    if let Err(e) = storage
                        .update_job_group_state(
                            *gid,
                            ci_core::models::job_group::JobGroupState::Expired,
                            Some(startup_reason),
                        )
                        .await
                    {
                        warn!("Failed to expire stale group {} in DB: {}", gid, e);
                    }
                    // Persist job cancellations for the expired group
                    if let Err(e) = storage.cancel_jobs_for_group(*gid).await {
                        warn!(
                            "Failed to cancel orphaned jobs in DB for stale group {}: {}",
                            gid, e
                        );
                    }
                    // Release the per-group Redis reservation key
                    if let (Some(redis), Some(wid)) = (&state.redis_store, worker_id.as_deref()) {
                        if let Err(e) = redis
                            .release_worker_reservation(wid, &gid.to_string())
                            .await
                        {
                            warn!(
                                "Failed to release Redis reservation for stale group {} worker {}: {}",
                                gid, wid, e
                            );
                        }
                    }
                }
                info!(
                    "Expired {} stale reserved groups on startup",
                    stale_ids.len()
                );
            }
        }

        // Recover jobs
        match storage.load_active_jobs().await {
            Ok(db_jobs) => {
                let mut jr = state.job_registry.write().await;
                for db_job in &db_jobs {
                    let job = db_job_to_job(db_job);
                    jr.add_job(job);
                }
                if !db_jobs.is_empty() {
                    info!("Recovered {} active jobs from DB", db_jobs.len());
                }
                // Mark any running/assigned jobs as Unknown — controller had no
                // live connection to the worker when it crashed.
                let stale = jr.mark_stale_jobs_unknown();
                if !stale.is_empty() {
                    warn!(
                        "Marked {} stale running/assigned jobs as Unknown",
                        stale.len()
                    );
                }
            }
            Err(e) => warn!("Failed to recover jobs: {}", e),
        }

        // Restore group_jobs mapping so the group registry knows which jobs
        // belong to each group (needed for completion checks, cancellation, etc.)
        match storage.load_active_jobs().await {
            Ok(db_jobs) => {
                let mut jgr = state.job_group_registry.write().await;
                let mut restored = 0usize;
                for db_job in &db_jobs {
                    let job = db_job_to_job(db_job);
                    jgr.add_job_to_group(&db_job.job_group_id, job);
                    restored += 1;
                }
                if restored > 0 {
                    info!("Restored {} jobs to group registries", restored);
                }
            }
            Err(e) => warn!("Failed to restore group_jobs: {}", e),
        }

        // Recover workers (mark all as Disconnected until they reconnect)
        match storage.load_workers().await {
            Ok(worker_rows) => {
                let mut wr = state.worker_registry.write().await;
                for row in &worker_rows {
                    let ws = worker_row_to_state(row);
                    wr.insert_worker_state(ws);
                }
                if !worker_rows.is_empty() {
                    info!("Recovered {} workers from DB", worker_rows.len());
                }
            }
            Err(e) => warn!("Failed to recover workers: {}", e),
        }

        // Restore worker resource allocations from active groups
        {
            let jgr = state.job_group_registry.read().await;
            let mut wr = state.worker_registry.write().await;
            let mut restored = 0usize;
            for group in jgr.active_groups() {
                if let Some(wid) = &group.reserved_worker_id {
                    let alloc = &group.allocated_resources;
                    if alloc.cpu > 0 || alloc.memory_mb > 0 || alloc.disk_mb > 0 {
                        if let Some(w) = wr.get_mut(wid) {
                            w.allocate(alloc.cpu, alloc.memory_mb, alloc.disk_mb);
                            restored += 1;
                        }
                    }
                }
            }
            if restored > 0 {
                info!(
                    "Restored resource allocations for {} active groups",
                    restored
                );
            }
        }

        // Reconcile Redis reservations against DB active groups
        if let Some(redis) = &state.redis_store {
            let active_groups = state.job_group_registry.read().await;
            let active = active_groups.active_groups();
            let mut orphaned = 0usize;

            // Check each active group has a matching per-group reservation key
            for group in active {
                if let Some(worker_id) = &group.reserved_worker_id {
                    let gid_str = group.id.to_string();
                    match redis.get_reservation_ttl(worker_id, &gid_str).await {
                        Ok(Some(_)) => {
                            // Key exists — consistent
                        }
                        Ok(None) => {
                            warn!(
                                "Redis reservation missing for worker {} group {} — will re-acquire on worker reconnect",
                                worker_id, group.id
                            );
                        }
                        Err(e) => warn!("Redis check failed for worker {}: {}", worker_id, e),
                    }
                }
            }

            // Scan ALL Redis reservation keys for orphaned per-group locks
            match redis.scan_all_reservations().await {
                Ok(reservations) => {
                    for (wid, group_id_str) in &reservations {
                        let group_uuid = group_id_str.parse::<uuid::Uuid>().ok();
                        let still_active = group_uuid.is_some_and(|gid| {
                            active_groups
                                .get(&gid)
                                .is_some_and(|g| !g.state.is_terminal())
                        });
                        if !still_active {
                            warn!(
                                "Orphaned Redis reservation for worker {} (group {}), releasing",
                                wid, group_id_str
                            );
                            if let Err(e) =
                                redis.release_worker_reservation(wid, group_id_str).await
                            {
                                warn!("Failed to release orphaned reservation for {}: {}", wid, e);
                            } else {
                                orphaned += 1;
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to scan Redis reservations: {}", e),
            }
            if orphaned > 0 {
                info!("Released {} orphaned Redis reservations", orphaned);
            }
            info!("Redis reconciliation complete");
        }

        // Clean up orphaned jobs whose groups are already terminal.
        // Catches any missed DB updates from previous crashes.
        let cleaned = storage.cleanup_orphaned_jobs().await.unwrap_or(0);
        if cleaned > 0 {
            info!("Cleaned up {} orphaned jobs with terminal groups", cleaned);
        }
    }
    // ── End state recovery ────────────────────────────────────────────────────

    // ── Retention cleanup background task ────────────────────────────────────
    let cancel_token = tokio_util::sync::CancellationToken::new();
    if let Some(retention_config) = config.retention.clone() {
        if state.storage.is_some() {
            let _retention = retention::spawn_cleanup_task(
                state.clone(),
                retention_config,
                cancel_token.clone(),
            );
            info!("Retention cleanup task started");
        }
    }

    // Reservation timeout reaper
    {
        let reaper_state = state.clone();
        let default_idle = config.workers.idle_timeout_secs;
        let default_stall = config.workers.stall_timeout_secs;
        let cancel_token_clone = cancel_token.clone();
        tokio::spawn(async move {
            info!(
                "Reservation reaper started (default idle={}s, stall={}s)",
                default_idle, default_stall
            );
            loop {
                tokio::select! {
                    _ = cancel_token_clone.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                }
                // Read DB overrides each cycle (settings page can change these at runtime)
                let (idle_timeout, stall_timeout) = if let Some(storage) = &reaper_state.storage {
                    let settings = storage.get_all_config_settings().await.unwrap_or_default();
                    let idle = settings
                        .get("workers.idle_timeout_secs")
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(default_idle);
                    let stall = settings
                        .get("workers.stall_timeout_secs")
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(default_stall);
                    (idle, stall)
                } else {
                    (default_idle, default_stall)
                };
                reap_stale_reservations(&reaper_state, idle_timeout, stall_timeout).await;
            }
            info!("Reservation reaper stopped");
        });
    }

    // Start HTTP sidecar in background
    let http_addr: std::net::SocketAddr = format!("0.0.0.0:{}", config.http_port).parse()?;
    {
        let http_state = state.clone();
        let http_tls = config.http_tls.clone();
        tokio::spawn(async move {
            if let Err(e) = http_server::run(http_addr, http_state, http_tls).await {
                error!("HTTP server failed: {}", e);
            }
        });
    }

    // Run gRPC server with graceful shutdown on SIGINT/SIGTERM
    tokio::select! {
        result = grpc_server::run(state) => {
            if let Err(e) = result {
                error!("Controller failed: {}", e);
                std::process::exit(1);
            }
        }
        _ = shutdown_signal() => {
            info!("Controller shutting down gracefully");
            cancel_token.cancel();
        }
    }

    Ok(())
}

/// Convert a DB job row into the in-memory `Job` struct used by the registries.
/// Resource requirements (cpu/memory/disk) are not stored on the job row — use 0
/// as a safe default; they're only needed at dispatch time and are re-read from
/// stage_configs then.
fn db_job_to_job(db: &storage::DbJob) -> Job {
    let job_type = JobType::Common; // default; not stored on the job row
    let mut job = Job::new(db.id.to_string(), db.command.clone(), job_type, 0, 0, 0);
    job.state = JobState::from_str(&db.state);
    job.assigned_worker = db.worker_id.clone();
    job.job_group_id = Some(db.job_group_id);
    job.stage_config_id = db.stage_config_id;
    job.stage_name = Some(db.stage_name.clone());
    job.pre_script = db.pre_script.clone();
    job.post_script = db.post_script.clone();
    job.exit_code = db.exit_code;
    job.pre_exit_code = db.pre_exit_code;
    job.post_exit_code = db.post_exit_code;
    job.log_path = db.log_path.clone();
    job.started_at = db.started_at;
    job.completed_at = db.completed_at;
    job.status_reason = db.status_reason.clone();
    job.created_at = db.created_at;
    job.updated_at = db.updated_at;
    job
}

/// Convert a DB worker row into a `WorkerState` for the registry.
/// All recovered workers start as `Disconnected` — they will transition to
/// `Connected` when they send their next heartbeat / re-register.
fn worker_row_to_state(row: &storage::WorkerRow) -> WorkerState {
    let disk_type = match row.disk_type.as_deref() {
        Some("nvme") => DiskType::Nvme,
        _ => DiskType::Sata,
    };
    let info = WorkerInfo {
        worker_id: row.worker_id.clone(),
        hostname: row.hostname.clone().unwrap_or_default(),
        total_cpu: row.total_cpu.unwrap_or(0) as u32,
        total_memory_mb: row.total_memory_mb.unwrap_or(0) as u64,
        total_disk_mb: row.total_disk_mb.unwrap_or(0) as u64,
        disk_type,
        supported_job_types: row.supported_job_types.clone().unwrap_or_default(),
        docker_enabled: row.docker_enabled,
        labels: Vec::new(),
        disk_details: Vec::new(),
    };
    WorkerState {
        info,
        status: WorkerStatus::Disconnected,
        last_heartbeat: None,
        registered_at: row.registered_at,
        system_info: row.system_info.clone(),
        allocated_cpu: 0,
        allocated_memory_mb: 0,
        allocated_disk_mb: 0,
    }
}

/// Reap job groups whose reservations have gone idle (no stage submitted)
/// or stalled (no activity while running). Releases worker resources and
/// Redis keys, then persists the failed state to the database.
async fn reap_stale_reservations(
    state: &Arc<ControllerState>,
    idle_timeout: u64,
    stall_timeout: u64,
) {
    let now = chrono::Utc::now();

    // Collect stale groups under read lock first.
    // Tuple: (group_id, worker_id, alloc, repo_id, branch, commit_sha, was_reserved)
    let stale: Vec<(
        uuid::Uuid,
        Option<String>,
        ci_core::models::job_group::AllocatedResources,
        Option<uuid::Uuid>,
        Option<String>,
        Option<String>,
        bool,
    )> = {
        let jg = state.job_group_registry.read().await;
        jg.active_groups()
            .iter()
            .filter(|g| {
                let idle_secs = (now - g.last_activity_at).num_seconds().max(0) as u64;
                match g.state {
                    ci_core::models::job_group::JobGroupState::Reserved => idle_secs > idle_timeout,
                    ci_core::models::job_group::JobGroupState::Running => {
                        // Don't timeout if any job is actively running —
                        // the stage has its own max_duration_secs timeout
                        let has_running_jobs = jg.get_jobs_for_group(&g.id).iter().any(|j| {
                            matches!(
                                j.state,
                                ci_core::models::job::JobState::Running
                                    | ci_core::models::job::JobState::Assigned
                            )
                        });
                        if has_running_jobs {
                            false // skip — stage is running, let stage timeout handle it
                        } else {
                            idle_secs > stall_timeout // no jobs running, apply stall timeout
                        }
                    }
                    _ => false,
                }
            })
            .map(|g| {
                let was_reserved = g.state == ci_core::models::job_group::JobGroupState::Reserved;
                (
                    g.id,
                    g.reserved_worker_id.clone(),
                    g.allocated_resources,
                    g.repo_id,
                    g.branch.clone(),
                    g.commit_sha.clone(),
                    was_reserved,
                )
            })
            .collect()
    };

    if stale.is_empty() {
        return;
    }

    for (group_id, worker_id, alloc, repo_id, branch, commit_sha, was_reserved) in &stale {
        let reap_reason = if *was_reserved {
            format!("Reservation expired: no stage submitted within {idle_timeout}s")
        } else {
            format!("Reservation expired: no activity for {stall_timeout}s after last stage")
        };
        warn!(
            "Reaping stale reservation: group={} worker={:?} reason={}",
            group_id, worker_id, reap_reason
        );

        {
            let mut jg = state.job_group_registry.write().await;
            jg.update_state(group_id, ci_core::models::job_group::JobGroupState::Expired);
            if let Some(g) = jg.get_mut(group_id) {
                g.status_reason = Some(reap_reason.clone());
            }
            jg.fail_group_jobs(group_id, &reap_reason);
        }

        // Dispatch global post-script (best-effort; worker may be disconnected)
        if let Some(wid) = worker_id {
            grpc_server::dispatch_global_post_script(
                state,
                group_id,
                wid,
                *repo_id,
                branch.clone(),
                commit_sha.clone(),
                ci_core::models::job_group::JobGroupState::Expired,
            )
            .await;
        }

        if let Some(wid) = worker_id {
            if alloc.cpu > 0 || alloc.memory_mb > 0 || alloc.disk_mb > 0 {
                let mut wr = state.worker_registry.write().await;
                if let Some(w) = wr.get_mut(wid) {
                    w.release(alloc.cpu, alloc.memory_mb, alloc.disk_mb);
                }
            }
            if let Some(redis) = &state.redis_store {
                if let Err(e) = reservation::ReservationManager::release(redis, wid, group_id).await
                {
                    warn!(
                        "Failed to release Redis for reaped group {}: {}",
                        group_id, e
                    );
                }
            }
        }

        if let Some(storage) = &state.storage {
            if let Err(e) = storage
                .update_job_group_state(
                    *group_id,
                    ci_core::models::job_group::JobGroupState::Expired,
                    Some(&reap_reason),
                )
                .await
            {
                warn!("Failed to persist reaped group {} to DB: {}", group_id, e);
            }
            // Persist job cancellations so they survive restarts
            if let Err(e) = storage.cancel_jobs_for_group(*group_id).await {
                warn!(
                    "Failed to cancel orphaned jobs in DB for reaped group {}: {}",
                    group_id, e
                );
            }
        }

        state.metrics.dec_active_builds();
    }

    info!("Reaped {} stale reservations", stale.len());
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => { warn!("Received SIGINT, shutting down..."); }
            _ = sigterm.recv() => { warn!("Received SIGTERM, shutting down..."); }
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.expect("failed to listen for Ctrl+C");
        warn!("Received Ctrl+C, shutting down...");
    }
}
