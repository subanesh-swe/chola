mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "chola-job-runner",
    about = "Chola CI Job Runner - Submit jobs and manage builds"
)]
struct Cli {
    /// Controller gRPC address (overrides config file)
    #[arg(short = 'C', long)]
    controller: Option<String>,

    /// Path to job-runner YAML config file
    /// Defaults: ~/.config/chola/job-runner.yaml → /etc/chola/job-runner.yaml
    #[arg(short, long)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Reserve a worker for a multi-stage build pipeline
    Reserve {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        repo_url: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        commit: Option<String>,
        #[arg(long, value_delimiter = ',')]
        stages: Vec<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Run a stage within a reserved job group
    Run {
        #[arg(long)]
        job_group_id: String,
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long)]
        stage: String,
        #[arg(long)]
        command_override: Option<String>,
    },
    /// Watch logs for a job group or specific stage
    Logs {
        #[arg(long)]
        job_group_id: Option<String>,
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long)]
        stage: Option<String>,
    },
    /// Cancel a job group or specific stage
    Cancel {
        #[arg(long)]
        job_group_id: Option<String>,
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long, default_value = "User requested cancellation")]
        reason: String,
    },
    /// Get status of a job group with all stages
    Status {
        #[arg(long)]
        job_group_id: String,
    },
    /// Submit a single job (legacy mode)
    Submit {
        #[arg(short = 'i', long, default_value = "job-001")]
        job_id: String,
        #[arg(short = 't', long, default_value = "common")]
        job_type: String,
        /// Command to execute
        #[arg(required = true)]
        command: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Simple job-runner config (optional YAML file)
#[derive(Debug, Default, serde::Deserialize)]
struct RunnerConfig {
    #[serde(default = "default_controller")]
    controller: String,
    #[serde(default)]
    token: Option<String>,
}

fn default_controller() -> String {
    "http://localhost:50051".to_string()
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt().with_env_filter("info").init();

    // Load config: --config flag → ~/.config/chola/job-runner.yaml → /etc/chola/job-runner.yaml
    let file_config = cli
        .config
        .or_else(|| ci_core::models::config::resolve_default_config("job-runner"))
        .and_then(|path| {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_yaml::from_str::<RunnerConfig>(&s).ok())
        })
        .unwrap_or_default();

    // Priority: CLI flag > config file > default
    let controller = cli.controller.unwrap_or(file_config.controller);
    // Priority: CHOLA_TOKEN env > config file token
    let token = std::env::var("CHOLA_TOKEN").ok().or(file_config.token);

    let mut client = commands::connect(&controller, token.as_deref()).await?;

    match cli.command {
        Commands::Reserve {
            repo,
            repo_url,
            branch,
            commit,
            stages,
            idempotency_key,
        } => {
            commands::reserve::execute(
                &mut client,
                repo,
                repo_url,
                branch,
                commit,
                stages,
                idempotency_key,
            )
            .await
        }

        Commands::Run {
            job_group_id,
            job_id,
            stage,
            command_override,
        } => {
            commands::run::execute(&mut client, job_group_id, job_id, stage, command_override).await
        }

        Commands::Logs {
            job_group_id,
            job_id,
            stage,
        } => commands::logs::execute(&mut client, job_group_id, job_id, stage).await,

        Commands::Cancel {
            job_group_id,
            job_id,
            reason,
        } => commands::cancel::execute(&mut client, job_group_id, job_id, reason).await,

        Commands::Status { job_group_id } => {
            commands::status::execute(&mut client, job_group_id).await
        }

        Commands::Submit {
            job_id,
            job_type,
            command,
        } => commands::submit::execute(&mut client, job_id, job_type, command).await,
    }
}
