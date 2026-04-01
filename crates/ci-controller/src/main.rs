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
    #[arg(short, long)]
    config: String,

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
    let mut config = ControllerConfig::from_file(&cli.config)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

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

    // Build log aggregator
    let log_agg = match &config.logging.log_dir {
        Some(dir) => LogAggregator::with_log_dir(dir.clone()),
        None => LogAggregator::new(),
    };

    // Build auth config for middleware
    let mut auth_config = AuthConfig::from_controller_config(&config.auth);
    auth_config.storage = storage.clone();

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

        // Reconcile Redis reservations against DB active groups
        if let Some(redis) = &state.redis_store {
            let active_groups = state.job_group_registry.read().await;
            let active = active_groups.active_groups();
            let mut orphaned = 0usize;
            for group in active {
                if let Some(worker_id) = &group.reserved_worker_id {
                    match redis.get_worker_reservation(worker_id).await {
                        Ok(Some(_)) => {
                            // Lock exists — consistent, nothing to do
                        }
                        Ok(None) => {
                            // Redis lock missing but DB says reserved — log only,
                            // worker will re-acquire on reconnect
                            warn!(
                                "Redis reservation missing for worker {} group {} — will re-acquire on worker reconnect",
                                worker_id, group.id
                            );
                        }
                        Err(e) => warn!("Redis check failed for worker {}: {}", worker_id, e),
                    }
                }
            }
            // Scan for orphaned Redis locks whose groups are no longer active
            let wr = state.worker_registry.read().await;
            for ws in wr.all_workers() {
                let wid = &ws.info.worker_id;
                match redis.get_worker_reservation(wid).await {
                    Ok(Some(group_id_str)) => {
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
                            if let Err(e) = redis.release_worker_force(wid).await {
                                warn!("Failed to release orphaned reservation for {}: {}", wid, e);
                            } else {
                                orphaned += 1;
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => warn!("Redis scan failed for worker {}: {}", wid, e),
                }
            }
            if orphaned > 0 {
                info!("Released {} orphaned Redis reservations", orphaned);
            }
            info!("Redis reconciliation complete");
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
