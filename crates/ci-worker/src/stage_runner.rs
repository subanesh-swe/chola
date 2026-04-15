use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::executor::{Executor, LogLine};

/// Lock configuration for a script, received from the controller.
#[derive(Debug, Clone, Default)]
pub struct ScriptLockConfig {
    pub enabled: bool,
    pub lock_key: String,
    pub timeout_secs: i32,
}

impl ScriptLockConfig {
    pub fn from_proto(proto: Option<ci_core::proto::orchestrator::ScriptLockConfig>) -> Self {
        match proto {
            Some(p) if p.enabled => Self {
                enabled: true,
                lock_key: p.lock_key,
                timeout_secs: p.timeout_secs,
            },
            _ => Self::default(),
        }
    }

    /// Resolve template variables in lock key.
    /// Supported: {{WORKER_ID}}, {{REPO_NAME}}, {{COMMIT_SHA}}, {{BRANCH}},
    ///            {{STAGE_NAME}}, {{JOB_GROUP_ID}}
    pub fn resolve_key(&self, env: &HashMap<String, String>) -> String {
        let mut key = self.lock_key.clone();
        let mappings = [
            ("{{WORKER_ID}}", "WORKER_ID"),
            ("{{REPO_NAME}}", "REPO_NAME"),
            ("{{COMMIT_SHA}}", "COMMIT_SHA"),
            ("{{BRANCH}}", "BRANCH"),
            ("{{STAGE_NAME}}", "STAGE_NAME"),
            ("{{JOB_GROUP_ID}}", "JOB_GROUP_ID"),
        ];
        for (template, env_key) in &mappings {
            if let Some(val) = env.get(*env_key) {
                key = key.replace(template, val);
            }
        }
        key
    }
}

/// Acquire a file lock (flock) with timeout. Returns the lock file handle on success.
/// Lock files are created under /tmp/chola-locks/.
async fn acquire_flock(
    resolved_key: &str,
    timeout_secs: i32,
    log_tx: &mpsc::Sender<LogLine>,
) -> anyhow::Result<std::fs::File> {
    use std::fs::{self, OpenOptions};

    let lock_dir = PathBuf::from("/tmp/chola-locks");
    fs::create_dir_all(&lock_dir)?;

    // Sanitize lock key for filename (max 200 chars to stay under ext4 255 limit)
    let safe_key: String = resolved_key
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(200)
        .collect();
    let lock_path = lock_dir.join(format!("{}.lock", safe_key));

    let _ = log_tx
        .send(LogLine {
            line: format!(
                "[LOCK] Acquiring lock: {} (timeout {}s)",
                resolved_key, timeout_secs
            ),
            is_stderr: false,
        })
        .await;

    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)?;

    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs.max(1) as u64);

    loop {
        // Try non-blocking flock
        let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
        let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 {
            let _ = log_tx
                .send(LogLine {
                    line: format!("[LOCK] Acquired: {}", resolved_key),
                    is_stderr: false,
                })
                .await;
            return Ok(file);
        }
        // Only retry on EWOULDBLOCK (lock held by another process).
        // Any other errno (EBADF, ENOLCK, etc.) is a real error — fail immediately.
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::WouldBlock {
            return Err(anyhow::anyhow!("flock failed: {}", err));
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "Lock timeout after {}s waiting for: {}",
                timeout_secs,
                resolved_key
            ));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Mask secret values in a command string for safe logging.
fn mask_command_secrets(command: &str, secrets: &[String]) -> String {
    let mut masked = command.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            masked = masked.replace(secret.as_str(), "****");
        }
    }
    masked
}

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
        s.replace("..", "").replace(['/', '\\'], "_")
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
    #[allow(clippy::too_many_arguments)]
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
        secret_values: Arc<Vec<String>>,
        environment: &HashMap<String, String>,
        pre_script_lock: &ScriptLockConfig,
        post_script_lock: &ScriptLockConfig,
    ) -> anyhow::Result<StageResult> {
        let mut pre_exit_code: Option<i32> = None;
        #[allow(unused_assignments)]
        let mut command_exit_code: i32 = -1;
        #[allow(unused_assignments)]
        let mut post_exit_code: Option<i32> = None;
        let mut was_cancelled = false;

        // -- Phase 1: Pre-script --
        if !pre_script.is_empty() {
            info!("Running pre_script");

            // Acquire lock if configured (held until _pre_lock_guard is dropped)
            let _pre_lock_guard = if pre_script_lock.enabled {
                let resolved = pre_script_lock.resolve_key(environment);
                match acquire_flock(&resolved, pre_script_lock.timeout_secs, &log_tx).await {
                    Ok(guard) => Some(guard),
                    Err(e) => {
                        error!("Pre-script lock failed: {}", e);
                        let _ = log_tx
                            .send(LogLine {
                                line: format!("[LOCK] Failed to acquire lock: {}", e),
                                is_stderr: true,
                            })
                            .await;
                        pre_exit_code = Some(-1);
                        command_exit_code = -1;
                        post_exit_code = self
                            .run_post_script(
                                post_script,
                                work_dir,
                                log_path,
                                &log_tx,
                                &secret_values,
                                environment,
                                command_exit_code,
                                false,
                                post_script_lock,
                            )
                            .await;
                        return Ok(StageResult {
                            pre_exit_code,
                            command_exit_code,
                            post_exit_code,
                            final_state: StageState::Failed,
                        });
                    }
                }
            } else {
                None
            };

            let _ = log_tx
                .send(LogLine {
                    line: format!(
                        "[PRE] Running pre-script ({} lines)...",
                        pre_script.lines().count()
                    ),
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
                    secret_values.clone(),
                    environment,
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
                        drop(_pre_lock_guard); // release pre-script lock before post-script
                        post_exit_code = self
                            .run_post_script(
                                post_script,
                                work_dir,
                                log_path,
                                &log_tx,
                                &secret_values,
                                environment,
                                command_exit_code,
                                false,
                                post_script_lock,
                            )
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
                    drop(_pre_lock_guard); // release pre-script lock before post-script
                    post_exit_code = self
                        .run_post_script(
                            post_script,
                            work_dir,
                            log_path,
                            &log_tx,
                            &secret_values,
                            environment,
                            -1,
                            false,
                            post_script_lock,
                        )
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

        // Log the command (with secrets masked)
        let masked_cmd = mask_command_secrets(command, &secret_values);
        let _ = log_tx
            .send(LogLine {
                line: format!("[CMD] Running: {}", masked_cmd),
                is_stderr: false,
            })
            .await;

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
                .execute_streaming(
                    command,
                    work_dir,
                    log_path,
                    log_tx.clone(),
                    merged_rx,
                    secret_values.clone(),
                    environment,
                )
                .await;

            // If command finished before timeout, cancel the timeout task
            timeout_handle.abort();

            r
        } else {
            self.executor
                .execute_streaming(
                    command,
                    work_dir,
                    log_path,
                    log_tx.clone(),
                    cancel_rx,
                    secret_values.clone(),
                    environment,
                )
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
            .run_post_script(
                post_script,
                work_dir,
                log_path,
                &log_tx,
                &secret_values,
                environment,
                command_exit_code,
                was_cancelled,
                post_script_lock,
            )
            .await;

        // The command's exit code is the sole determinant of success/failure.
        // Post-script is cleanup — its exit code is tracked but never affects
        // the stage outcome.
        let final_state = if was_cancelled {
            StageState::Cancelled
        } else if command_exit_code != 0 {
            StageState::Failed
        } else {
            StageState::Success
        };

        Ok(StageResult {
            pre_exit_code,
            command_exit_code,
            post_exit_code,
            final_state,
        })
    }

    /// Run post_script. Returns exit code or None if no post_script.
    /// Injects STAGE_EXIT_CODE and STAGE_WAS_CANCELLED env vars so the
    /// post-script can behave differently on abort vs success.
    #[allow(clippy::too_many_arguments)]
    async fn run_post_script(
        &self,
        post_script: &str,
        work_dir: &str,
        log_path: &str,
        log_tx: &mpsc::Sender<LogLine>,
        secret_values: &Arc<Vec<String>>,
        environment: &HashMap<String, String>,
        command_exit_code: i32,
        was_cancelled: bool,
        lock_config: &ScriptLockConfig,
    ) -> Option<i32> {
        if post_script.is_empty() {
            return None;
        }

        info!("Running post_script (MUST complete)");

        // Acquire lock if configured
        let _post_lock_guard = if lock_config.enabled {
            let resolved = lock_config.resolve_key(environment);
            match acquire_flock(&resolved, lock_config.timeout_secs, log_tx).await {
                Ok(guard) => Some(guard),
                Err(e) => {
                    warn!("Post-script lock failed: {}", e);
                    let _ = log_tx
                        .send(LogLine {
                            line: format!("[LOCK] Post-script lock failed: {}", e),
                            is_stderr: true,
                        })
                        .await;
                    // Post-script lock failure is not fatal — run without lock
                    None
                }
            }
        } else {
            None
        };

        let _ = log_tx
            .send(LogLine {
                line: format!(
                    "[POST] Running post-script ({} lines)...",
                    post_script.lines().count()
                ),
                is_stderr: false,
            })
            .await;

        // Inject stage outcome so post-script can branch on success/failure/cancel
        let mut post_env = environment.clone();
        post_env.insert("STAGE_EXIT_CODE".into(), command_exit_code.to_string());
        post_env.insert(
            "STAGE_WAS_CANCELLED".into(),
            if was_cancelled { "true" } else { "false" }.into(),
        );

        let (_dummy_cancel_tx, dummy_cancel_rx) = mpsc::channel(1);
        match self
            .executor
            .execute_streaming(
                post_script,
                work_dir,
                log_path,
                log_tx.clone(),
                dummy_cancel_rx,
                secret_values.clone(),
                &post_env,
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
