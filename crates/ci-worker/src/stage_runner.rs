use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::executor::{ExecutionResult, Executor, LogLine};

/// Result of a stage execution
#[derive(Debug)]
pub struct StageResult {
    pub pre_exit_code: Option<i32>,
    pub command_exit_code: i32,
    pub post_exit_code: Option<i32>,
    pub final_state: StageState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StageState {
    Success,
    Failed,
    Cancelled,
}

/// Runs a complete stage: pre_script -> command -> post_script
pub struct StageRunner {
    executor: Executor,
}

impl StageRunner {
    pub fn new() -> Self {
        Self {
            executor: Executor::new(),
        }
    }

    /// Sanitize a path component to prevent directory traversal.
    fn sanitize_path_component(s: &str) -> String {
        s.replace("..", "").replace('/', "_").replace('\\', "_")
    }

    /// Determine the log path for a stage
    pub fn log_path(log_dir: &str, job_group_id: &str, stage_name: &str, job_id: &str) -> PathBuf {
        if !job_group_id.is_empty() && !stage_name.is_empty() {
            let safe_group = Self::sanitize_path_component(job_group_id);
            let safe_stage = Self::sanitize_path_component(stage_name);
            PathBuf::from(log_dir)
                .join(safe_group)
                .join(format!("{}.log", safe_stage))
        } else {
            let safe_job = Self::sanitize_path_component(job_id);
            PathBuf::from(log_dir).join(format!("{}.log", safe_job))
        }
    }

    /// Execute a full stage with pre/post scripts
    pub async fn run_stage(
        &self,
        command: &str,
        pre_script: &str,
        post_script: &str,
        work_dir: &str,
        log_path: &str,
        log_tx: mpsc::Sender<LogLine>,
        cancel_rx: mpsc::Receiver<i32>,
        max_duration_secs: i32,
    ) -> anyhow::Result<StageResult> {
        let mut pre_exit_code: Option<i32> = None;
        let mut command_exit_code: i32 = -1;
        let mut post_exit_code: Option<i32> = None;
        let mut was_cancelled = false;

        // -- Phase 1: Pre-script --
        if !pre_script.is_empty() {
            info!("Running pre_script");
            let _ = log_tx
                .send(LogLine {
                    line: "[PRE] Running pre-script...".to_string(),
                    is_stderr: false,
                })
                .await;

            // Create a dummy cancel_rx for pre_script (no cancellation during pre)
            let (_dummy_cancel_tx, dummy_cancel_rx) = mpsc::channel(1);
            match self
                .executor
                .execute_streaming(
                    pre_script,
                    work_dir,
                    log_path,
                    log_tx.clone(),
                    dummy_cancel_rx,
                )
                .await
            {
                Ok(result) => {
                    pre_exit_code = Some(result.exit_code);
                    if result.exit_code != 0 {
                        warn!("Pre-script failed with exit code {}", result.exit_code);
                        let _ = log_tx
                            .send(LogLine {
                                line: format!(
                                    "[PRE] Pre-script failed (exit code {})",
                                    result.exit_code
                                ),
                                is_stderr: true,
                            })
                            .await;
                        // Skip command, but still run post_script
                        command_exit_code = -1;
                        post_exit_code = self
                            .run_post_script(post_script, work_dir, log_path, &log_tx)
                            .await;
                        return Ok(StageResult {
                            pre_exit_code,
                            command_exit_code,
                            post_exit_code,
                            final_state: StageState::Failed,
                        });
                    }
                    let _ = log_tx
                        .send(LogLine {
                            line: "[PRE] Pre-script completed successfully".to_string(),
                            is_stderr: false,
                        })
                        .await;
                }
                Err(e) => {
                    error!("Pre-script execution error: {}", e);
                    pre_exit_code = Some(-1);
                    post_exit_code = self
                        .run_post_script(post_script, work_dir, log_path, &log_tx)
                        .await;
                    return Ok(StageResult {
                        pre_exit_code,
                        command_exit_code: -1,
                        post_exit_code,
                        final_state: StageState::Failed,
                    });
                }
            }
        }

        // -- Phase 2: Main command --
        info!("Running main command");

        // If there is a timeout, we merge the external cancel signal and a
        // timeout-generated signal into a single channel so that the executor's
        // existing cancellation logic (process tree kill) handles both cases.
        let result = if max_duration_secs > 0 {
            let (merged_tx, merged_rx) = mpsc::channel::<i32>(1);

            // Spawn timeout task: sends SIGTERM through the channel when deadline
            // is exceeded.  The executor will receive this exactly like an external
            // cancel and kill the process tree properly.
            let timeout_tx = merged_tx.clone();
            let timeout_secs = max_duration_secs as u64;
            let timeout_handle = tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(timeout_secs)).await;
                let _ = timeout_tx.send(15).await; // SIGTERM
            });

            // Spawn forwarder: relays external cancel signal to merged channel
            let forwarder_tx = merged_tx;
            let mut cancel_rx = cancel_rx;
            tokio::spawn(async move {
                if let Some(sig) = cancel_rx.recv().await {
                    let _ = forwarder_tx.send(sig).await;
                }
            });

            let r = self
                .executor
                .execute_streaming(command, work_dir, log_path, log_tx.clone(), merged_rx)
                .await;

            // If command finished before timeout, cancel the timeout task
            timeout_handle.abort();

            r
        } else {
            self.executor
                .execute_streaming(command, work_dir, log_path, log_tx.clone(), cancel_rx)
                .await
        };

        match result {
            Ok(r) => {
                command_exit_code = r.exit_code;
                if r.exit_code < 0 {
                    was_cancelled = true;
                }
            }
            Err(e) => {
                error!("Command execution error: {}", e);
                command_exit_code = -1;
            }
        }

        // -- Phase 3: Post-script (ALWAYS runs) --
        post_exit_code = self
            .run_post_script(post_script, work_dir, log_path, &log_tx)
            .await;

        let final_state = if was_cancelled {
            StageState::Cancelled
        } else if command_exit_code == 0 {
            StageState::Success
        } else {
            StageState::Failed
        };

        Ok(StageResult {
            pre_exit_code,
            command_exit_code,
            post_exit_code,
            final_state,
        })
    }

    /// Run post_script. Returns exit code or None if no post_script.
    async fn run_post_script(
        &self,
        post_script: &str,
        work_dir: &str,
        log_path: &str,
        log_tx: &mpsc::Sender<LogLine>,
    ) -> Option<i32> {
        if post_script.is_empty() {
            return None;
        }

        info!("Running post_script (MUST complete)");
        let _ = log_tx
            .send(LogLine {
                line: "[POST] Running post-script...".to_string(),
                is_stderr: false,
            })
            .await;

        let (_dummy_cancel_tx, dummy_cancel_rx) = mpsc::channel(1);
        match self
            .executor
            .execute_streaming(
                post_script,
                work_dir,
                log_path,
                log_tx.clone(),
                dummy_cancel_rx,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code != 0 {
                    warn!("Post-script failed with exit code {}", result.exit_code);
                    let _ = log_tx
                        .send(LogLine {
                            line: format!(
                                "[POST] Post-script failed (exit code {})",
                                result.exit_code
                            ),
                            is_stderr: true,
                        })
                        .await;
                } else {
                    let _ = log_tx
                        .send(LogLine {
                            line: "[POST] Post-script completed successfully".to_string(),
                            is_stderr: false,
                        })
                        .await;
                }
                Some(result.exit_code)
            }
            Err(e) => {
                error!("Post-script execution error: {}", e);
                let _ = log_tx
                    .send(LogLine {
                        line: format!("[POST] Post-script error: {}", e),
                        is_stderr: true,
                    })
                    .await;
                Some(-1)
            }
        }
    }
}
