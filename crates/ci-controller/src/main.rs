use clap::Parser;
use tracing::{error, info};

mod grpc_server;
mod job_registry;
mod log_aggregator;
mod monitoring;
mod redis_store;
mod scheduler;
mod storage;
mod worker_registry;

use ci_core::models::config::ControllerConfig;

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
    info!("Scheduling strategy: {}", config.scheduling.strategy);

    // Start gRPC server
    if let Err(e) = grpc_server::run(config).await {
        error!("Controller failed: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
