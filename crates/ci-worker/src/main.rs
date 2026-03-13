use clap::Parser;
use tracing::{error, info};

mod agent;
mod executor;
mod grpc_client;
mod heartbeat;
mod log_streamer;
mod reconnect;

use ci_core::models::config::WorkerConfig;

/// CI Orchestrator - Worker Agent
#[derive(Parser, Debug)]
#[command(name = "ci-worker", about = "CI Orchestrator Worker Agent")]
struct Cli {
    /// Path to worker YAML config file
    #[arg(short, long)]
    config: String,

    /// Override controller address
    #[arg(long)]
    controller_addr: Option<String>,

    /// Override worker ID
    #[arg(long)]
    worker_id: Option<String>,

    /// Override log level
    #[arg(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let mut config = WorkerConfig::from_file(&cli.config)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    // Apply CLI overrides
    if let Some(addr) = cli.controller_addr {
        config.controller.address = addr;
    }
    if let Some(id) = cli.worker_id {
        config.worker_id = id;
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

    // Start the worker agent
    if let Err(e) = agent::run(config).await {
        error!("Worker failed: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
