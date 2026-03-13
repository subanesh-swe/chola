use std::time::Duration;

use ci_core::proto::orchestrator::{
    JobState, LogResumeDirective, ReconnectRequest, ReconnectResponse, RunningJobInfo,
};
// Note: LogResumeDirective is used in tests
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Configuration for reconnect behavior
#[derive(Clone, Debug)]
pub struct ReconnectConfig {
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
    /// Maximum number of retry attempts (0 = infinite)
    pub max_attempts: u32,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            multiplier: 1.5,
            max_attempts: 0, // infinite retries
        }
    }
}

/// State of jobs currently running on the worker
#[derive(Clone, Debug, Default)]
pub struct WorkerJobState {
    /// List of running jobs with their current state
    pub running_jobs: Vec<RunningJobInfo>,
}

/// Result of a reconnect attempt
#[derive(Debug)]
pub struct ReconnectResult {
    /// The response from the controller
    pub response: ReconnectResponse,
    /// Number of attempts made
    pub attempts: u32,
    /// Total time spent reconnecting
    pub total_duration: Duration,
}

/// Handler for reconnection logic with exponential backoff
pub struct ReconnectHandler {
    config: ReconnectConfig,
    /// Current job state tracked by the worker
    job_state: std::sync::Arc<tokio::sync::RwLock<WorkerJobState>>,
}

impl ReconnectHandler {
    pub fn new() -> Self {
        Self::with_config(ReconnectConfig::default())
    }

    pub fn with_config(config: ReconnectConfig) -> Self {
        Self {
            config,
            job_state: std::sync::Arc::new(tokio::sync::RwLock::new(WorkerJobState::default())),
        }
    }

    /// Get a clone of the job state for tracking
    pub fn job_state(&self) -> std::sync::Arc<tokio::sync::RwLock<WorkerJobState>> {
        self.job_state.clone()
    }

    /// Register a job as active
    pub async fn register_job(&self, job_id: String, log_offset: u64) {
        let mut state = self.job_state.write().await;
        // Remove existing entry for this job if any
        state.running_jobs.retain(|j| j.job_id != job_id);
        state.running_jobs.push(RunningJobInfo {
            job_id,
            state: JobState::Running as i32,
            log_offset,
        });
        info!("Registered active job in reconnect handler");
    }

    /// Mark a job as completed (remove from active)
    pub async fn complete_job(&self, job_id: &str) {
        let mut state = self.job_state.write().await;
        let initial_len = state.running_jobs.len();
        state.running_jobs.retain(|j| j.job_id != job_id);
        if state.running_jobs.len() < initial_len {
            info!("Removed completed job {} from reconnect handler", job_id);
        }
    }

    /// Update the log offset for an active job
    pub async fn update_job_offset(&self, job_id: &str, log_offset: u64) {
        let mut state = self.job_state.write().await;
        if let Some(job) = state.running_jobs.iter_mut().find(|j| j.job_id == job_id) {
            job.log_offset = log_offset;
        }
    }

    /// Build a reconnect request with current worker state
    pub async fn build_request(&self, worker_id: String) -> ReconnectRequest {
        let state = self.job_state.read().await;
        ReconnectRequest {
            worker_id,
            running_jobs: state.running_jobs.clone(),
        }
    }

    /// Attempt to reconnect with exponential backoff.
    ///
    /// This function will:
    /// 1. Build a reconnect request with current job state
    /// 2. Attempt to call the controller's reconnect RPC
    /// 3. Retry with exponential backoff on failure
    /// 4. Return the response when successful or give up after max_attempts
    pub async fn reconnect<F, Fut>(
        &self,
        worker_id: String,
        mut reconnect_fn: F,
    ) -> anyhow::Result<ReconnectResult>
    where
        F: FnMut(ReconnectRequest) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<ReconnectResponse>>,
    {
        let start = std::time::Instant::now();
        let mut current_backoff = self.config.initial_backoff;
        let mut attempts = 0u32;

        let request = self.build_request(worker_id.clone()).await;
        info!(
            worker_id = %worker_id,
            running_jobs = request.running_jobs.len(),
            "Starting reconnect attempt with state"
        );

        loop {
            attempts += 1;

            match reconnect_fn(request.clone()).await {
                Ok(response) => {
                    info!(
                        worker_id = %worker_id,
                        attempts = attempts,
                        duration_ms = start.elapsed().as_millis(),
                        "Reconnect successful"
                    );
                    return Ok(ReconnectResult {
                        response,
                        attempts,
                        total_duration: start.elapsed(),
                    });
                }
                Err(e) => {
                    // Check if we've exceeded max attempts
                    if self.config.max_attempts > 0 && attempts >= self.config.max_attempts {
                        error!(
                            worker_id = %worker_id,
                            attempts = attempts,
                            error = %e,
                            "Reconnect failed after max attempts"
                        );
                        return Err(anyhow::anyhow!(
                            "Reconnect failed after {} attempts: {}",
                            attempts,
                            e
                        ));
                    }

                    warn!(
                        worker_id = %worker_id,
                        attempt = attempts,
                        backoff_ms = current_backoff.as_millis(),
                        error = %e,
                        "Reconnect attempt failed, backing off before retry"
                    );

                    sleep(current_backoff).await;

                    // Calculate next backoff with multiplier
                    current_backoff = std::cmp::min(
                        Duration::from_secs_f64(
                            current_backoff.as_secs_f64() * self.config.multiplier,
                        ),
                        self.config.max_backoff,
                    );
                }
            }
        }
    }

    /// Process log resume directives from reconnect response.
    ///
    /// Returns a list of (job_id, resume_offset) pairs for jobs that need log resumption.
    pub fn process_resume_directives(response: &ReconnectResponse) -> Vec<(String, u64)> {
        response
            .log_resumes
            .iter()
            .map(|directive| (directive.job_id.clone(), directive.resume_from_offset))
            .collect()
    }
}

impl Default for ReconnectHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reconnect_handler_tracks_jobs() {
        let handler = ReconnectHandler::new();

        handler.register_job("job-1".to_string(), 0).await;
        handler.register_job("job-2".to_string(), 1024).await;

        let state = handler.job_state.read().await;
        assert_eq!(state.running_jobs.len(), 2);

        drop(state);
        handler.complete_job("job-1").await;

        let state = handler.job_state.read().await;
        assert_eq!(state.running_jobs.len(), 1);
        assert_eq!(state.running_jobs[0].job_id, "job-2");
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff() {
        let config = ReconnectConfig {
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            multiplier: 2.0,
            max_attempts: 3,
        };
        let handler = ReconnectHandler::with_config(config);

        let result = handler
            .reconnect("worker-1".to_string(), |_req| async {
                Err(anyhow::anyhow!("Simulated failure"))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Reconnect failed after 3 attempts: Simulated failure"
        );
    }

    #[tokio::test]
    async fn test_reconnect_success_after_failures() {
        let config = ReconnectConfig {
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            multiplier: 2.0,
            max_attempts: 0, // infinite
        };
        let handler = ReconnectHandler::with_config(config);

        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempt_count_clone = attempt_count.clone();

        let result = handler
            .reconnect("worker-1".to_string(), move |_req| {
                let count = attempt_count_clone.clone();
                async move {
                    let current = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if current < 2 {
                        Err(anyhow::anyhow!("Simulated failure"))
                    } else {
                        Ok(ReconnectResponse {
                            log_resumes: vec![],
                            accepted: true,
                            message: "Welcome back".to_string(),
                        })
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().attempts, 3);
    }

    #[test]
    fn test_process_resume_directives() {
        let response = ReconnectResponse {
            log_resumes: vec![
                LogResumeDirective {
                    job_id: "job-1".to_string(),
                    resume_from_offset: 1024,
                },
                LogResumeDirective {
                    job_id: "job-2".to_string(),
                    resume_from_offset: 2048,
                },
            ],
            accepted: true,
            message: "Welcome back".to_string(),
        };

        let directives = ReconnectHandler::process_resume_directives(&response);
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0], ("job-1".to_string(), 1024));
        assert_eq!(directives[1], ("job-2".to_string(), 2048));
    }
}
