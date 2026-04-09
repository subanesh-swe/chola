use clap::Parser;
use tracing::{error, info, warn};

mod agent;
mod executor;
mod grpc_client;
mod heartbeat;
mod http_server;
mod log_streamer;
mod reconnect;
mod stage_runner;

use ci_core::models::config::WorkerConfig;

/// CI Orchestrator - Worker Agent
#[derive(Parser, Debug)]
#[command(name = "ci-worker", about = "CI Orchestrator Worker Agent")]
struct Cli {
    /// Path to worker YAML config file
    /// Path to worker YAML config file
    /// Defaults: ~/.config/chola/worker.yaml → /etc/chola/worker.yaml
    #[arg(short, long)]
    config: Option<String>,

    /// Override controller address
    #[arg(long)]
    controller_addr: Option<String>,

    /// Override worker ID
    #[arg(long)]
    worker_id: Option<String>,

    /// Override log level
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
        .or_else(|| ci_core::models::config::resolve_default_config("worker"))
        .ok_or_else(|| anyhow::anyhow!(
            "No config file found. Pass --config or create ~/.config/chola/worker.yaml or /etc/chola/worker.yaml"
        ))?;
    let mut config = WorkerConfig::from_file(&config_path)
        .map_err(|e| anyhow::anyhow!("Failed to load config '{}': {}", config_path, e))?;

    // Apply CLI overrides
    if let Some(addr) = cli.controller_addr {
        config.controller.address = addr;
    }
    if let Some(id) = cli.worker_id {
        config.worker_id = id;
    }
    if let Some(port) = cli.http_port {
        config.http_port = port;
    }

    // Env var override for auth token
    if let Ok(val) = std::env::var("CHOLA_TOKEN") {
        config.token = Some(val);
    }

    let log_level = cli.log_level.as_deref().unwrap_or(&config.logging.level);

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    info!("Starting CI Worker: {}", config.worker_id);
    info!("Controller address: {}", config.controller.address);
    info!("HTTP port: {}", config.http_port);

    // Create shared worker metrics
    let worker_metrics = http_server::WorkerMetrics::new();
    let metrics_for_http = worker_metrics.clone();

    // Start HTTP health server in background
    let http_addr: std::net::SocketAddr = format!("0.0.0.0:{}", config.http_port).parse()?;
    tokio::spawn(async move {
        if let Err(e) = http_server::run(http_addr, metrics_for_http).await {
            error!("Worker HTTP server failed: {}", e);
        }
    });

    // Run worker agent with graceful shutdown on SIGINT/SIGTERM
    tokio::select! {
        result = agent::run(config, Some(worker_metrics)) => {
            if let Err(e) = result {
                error!("Worker failed: {}", e);
                std::process::exit(1);
            }
        }
        _ = shutdown_signal() => {
            info!("Worker shutting down gracefully");
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
