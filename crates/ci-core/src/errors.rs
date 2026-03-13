use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Worker not found: {0}")]
    WorkerNotFound(String),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Redis error: {0}")]
    Redis(String),

    #[error("Lock acquisition failed: {0}")]
    LockFailed(String),

    #[error("Worker already registered: {0}")]
    WorkerAlreadyRegistered(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;
