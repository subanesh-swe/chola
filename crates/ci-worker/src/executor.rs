use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

/// A single line of log output from the executor
#[derive(Debug, Clone)]
pub struct LogLine {
    pub line: String,
    pub is_stderr: bool,
}

/// Result of a job execution
pub struct ExecutionResult {
    pub exit_code: i32,
    pub output: String,
}

/// Running job state for cancellation
pub struct RunningJob {
    /// Process ID (negative for process group)
    pid: i32,
    /// Whether the job has been cancelled
    cancelled: bool,
}

/// Job executor — spawns commands, captures output, supports cancellation
pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self
    }

    /// Simple execution — waits for completion and returns all output.
    /// Used when log streaming is not wired up.
    pub async fn execute(&self, command: &str, work_dir: &str) -> anyhow::Result<ExecutionResult> {
        info!("Executing command: {}", command);

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()
            .await?;

        let exit_code = output.status.code().unwrap_or(-1);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut combined_output = String::new();
        if !stdout.is_empty() {
            combined_output.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !combined_output.is_empty() {
                combined_output.push('\n');
            }
            combined_output.push_str(&stderr);
        }

        if !output.status.success() {
            error!("Command failed with exit code: {}", exit_code);
        }

        Ok(ExecutionResult {
            exit_code,
            output: combined_output,
        })
    }

    /// Streaming execution — reads stdout/stderr line-by-line and sends through channel.
    ///
    /// Each line is sent as a `LogLine` through `log_tx` for real-time streaming.
    /// Lines are also appended to a local log file at `log_path` for reconnect replay.
    /// The channel is closed when the process exits.
    ///
    /// Supports cancellation via the cancel_rx channel. When a signal is received,
    /// the specified signal is sent to the entire process group.
    ///
    /// Returns exit code only (output is in the stream, not buffered).
    pub async fn execute_streaming(
        &self,
        command: &str,
        work_dir: &str,
        log_path: &str,
        log_tx: mpsc::Sender<LogLine>,
        mut cancel_rx: mpsc::Receiver<i32>,
    ) -> anyhow::Result<ExecutionResult> {
        info!("Executing command (streaming): {}", command);

        // Use process group(0) to create a new process group
        // This allows us to kill all child processes on cancel
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0) // Create new process group
            .spawn()?;

        let pid = child.id().unwrap_or(0) as i32;
        let running_job = Arc::new(Mutex::new(RunningJob {
            pid,
            cancelled: false,
        }));

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        // Ensure log directory exists
        if let Some(parent) = Path::new(log_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Open local log file for append (used for reconnect replay)
        let log_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .await?;
        let log_file = Arc::new(Mutex::new(log_file));

        // Clone for cancellation check
        let running_job_clone = running_job.clone();

        // Read stdout line-by-line in a spawned task
        let stdout_tx = log_tx.clone();
        let stdout_log_file = log_file.clone();
        let running_job_stdout = running_job_clone.clone();
        let stdout_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // Check if cancelled
                if running_job_stdout.lock().await.cancelled {
                    break;
                }
                // Write to local log file
                {
                    let mut f = stdout_log_file.lock().await;
                    let _ = f.write_all(line.as_bytes()).await;
                    let _ = f.write_all(b"\n").await;
                    let _ = f.flush().await;
                }
                // Send to log streamer channel
                if stdout_tx
                    .send(LogLine {
                        line,
                        is_stderr: false,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Read stderr line-by-line in a spawned task
        let stderr_tx = log_tx.clone();
        let stderr_log_file = log_file.clone();
        let running_job_stderr = running_job_clone;
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                // Check if cancelled
                if running_job_stderr.lock().await.cancelled {
                    break;
                }
                // Write to local log file
                {
                    let mut f = stderr_log_file.lock().await;
                    let _ = f.write_all(line.as_bytes()).await;
                    let _ = f.write_all(b"\n").await;
                    let _ = f.flush().await;
                }
                // Send to log streamer channel
                if stderr_tx
                    .send(LogLine {
                        line,
                        is_stderr: true,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Drop our copy of the sender so the channel closes when both tasks finish
        drop(log_tx);

        // Wait for process to exit or cancellation
        let exit_code = tokio::select! {
            // Normal process exit
            result = child.wait() => {
                match result {
                    Ok(status) => {
                        let code = status.code().unwrap_or(-1);
                        if !status.success() {
                            error!("Command failed with exit code: {}", code);
                        }
                        code
                    }
                    Err(e) => {
                        error!("Failed to wait for process: {}", e);
                        -1
                    }
                }
            }

            // Cancellation signal - receives the signal to send
            Some(signal) = cancel_rx.recv() => {
                info!("Received cancel signal for job, sending signal {} to process group {}", signal, -pid);

                // Mark as cancelled
                running_job.lock().await.cancelled = true;

                // Kill the entire process group
                // Use negative PID to kill the process group
                let pgid = -pid;

                // Send the requested signal (e.g., SIGINT=2, SIGTERM=15, SIGKILL=9)
                let signal_result = unsafe { libc::kill(pgid, signal) };
                if signal_result != 0 {
                    warn!("Signal {} failed for process group {}", signal, pgid);
                } else {
                    info!("Successfully sent signal {} to process group {}", signal, pgid);
                }

                // If not SIGKILL, give processes a moment to exit gracefully
                if signal != libc::SIGKILL {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                    // Check if still running, then force kill
                    let check_result = unsafe { libc::kill(pgid, 0) };
                    if check_result == 0 {
                        warn!("Process group still alive after signal {}, sending SIGKILL", signal);
                        let _ = unsafe { libc::kill(pgid, libc::SIGKILL) };
                    }
                }

                // Wait for process to be reaped
                match child.wait().await {
                    Ok(status) => {
                        info!("Process terminated after cancel: {:?}", status);
                    }
                    Err(e) => {
                        warn!("Error waiting for cancelled process: {}", e);
                    }
                }

                // Return negative of the signal to indicate cancellation
                -signal
            }
        };

        // Wait for both readers to finish (they may have stopped due to cancellation)
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), stdout_handle).await;
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), stderr_handle).await;

        Ok(ExecutionResult {
            exit_code,
            output: String::new(), // output is in the stream, not buffered
        })
    }
}
