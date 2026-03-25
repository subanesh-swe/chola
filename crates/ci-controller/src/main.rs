use std::sync::Arc;

use clap::Parser;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod grpc_server;
mod http_server;
mod job_group_registry;
mod job_registry;
mod log_aggregator;
mod monitoring;
mod redis_store;
mod reservation;
mod scheduler;
mod storage;
mod worker_registry;

use ci_core::models::config::ControllerConfig;
use job_group_registry::JobGroupRegistry;
use monitoring::Metrics;
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

    info!("Starting CI Controller");
    info!("Bind address: {}", config.bind_address);
    info!("HTTP port: {}", config.http_port);
    info!("Scheduling strategy: {}", config.scheduling.strategy);

    // Create shared registries
    let worker_registry = Arc::new(RwLock::new(WorkerRegistry::new()));
    let job_group_registry = Arc::new(RwLock::new(JobGroupRegistry::new()));
    let metrics = Metrics::new();

    // Start HTTP sidecar in background
    let http_addr: std::net::SocketAddr = format!("0.0.0.0:{}", config.http_port).parse()?;
    {
        let wr = worker_registry.clone();
        let jgr = job_group_registry.clone();
        let m = metrics.clone();
        tokio::spawn(async move {
            if let Err(e) = http_server::run(http_addr, wr, jgr, m).await {
                error!("HTTP server failed: {}", e);
            }
        });
    }

    // Run gRPC server with graceful shutdown on SIGINT/SIGTERM
    tokio::select! {
        result = grpc_server::run(config, worker_registry, job_group_registry, metrics) => {
            if let Err(e) = result {
                error!("Controller failed: {}", e);
                std::process::exit(1);
            }
        }
        _ = shutdown_signal() => {
            info!("Controller shutting down gracefully");
        }
    }

    Ok(())
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
