use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

// ── Process-tree termination helpers ────────────────────────────────────────

/// Walk `/proc` to collect every descendant PID of `root_pid` (breadth-first).
///
/// Reading `/proc/<n>/status` for every numeric entry in `/proc` is O(n) in the
/// total number of processes on the machine, but it is correct across process-group
/// boundaries — unlike `kill(-pgid, sig)` which misses descendants that called
/// `setpgid`/`setsid` (e.g. nix build sandboxes).
fn collect_descendants(root_pid: i32) -> Vec<i32> {
    let mut result: Vec<i32> = Vec::new();
    // frontier: PIDs whose children we still need to discover
    let mut frontier: Vec<i32> = vec![root_pid];

    while !frontier.is_empty() {
        let mut next_frontier: Vec<i32> = Vec::new();
        // Snapshot /proc once per generation
        let Ok(proc_dir) = std::fs::read_dir("/proc") else {
            break;
        };
        for entry in proc_dir.flatten() {
            let name = entry.file_name();
            let Ok(child_pid) = name.to_string_lossy().parse::<i32>() else {
                continue;
            };
            if child_pid == root_pid {
                continue; // skip root itself
            }
            let status_path = format!("/proc/{}/status", child_pid);
            let Ok(status) = std::fs::read_to_string(&status_path) else {
                continue; // process may have already exited
            };
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("PPid:") {
                    let ppid: i32 = rest.trim().parse().unwrap_or(0);
                    if frontier.contains(&ppid) {
                        result.push(child_pid);
                        next_frontier.push(child_pid);
                    }
                    break;
                }
            }
        }
        frontier = next_frontier;
    }

    result
}

/// Send `signal` to the process and all its descendants, wait for them to exit,
/// then force-kill any survivors with SIGKILL — mirroring the Python psutil pattern.
///
/// This does NOT reap the direct child (call `child.wait()` yourself after this
/// returns, since only the parent process may call waitpid on a child).
async fn terminate_process_tree(pid: i32, signal: i32) {
    info!(
        "Sending signal {} to process tree (root PID: {})",
        signal, pid
    );

    // Snapshot all descendants BEFORE signalling — some may exit and their PIDs
    // may be reused if we wait until after signals are sent.
    let descendants = collect_descendants(pid);
    info!(
        "Found {} descendant process(es) under PID {}",
        descendants.len(),
        pid
    );

    // ── 1. Signal the root process ──────────────────────────────────────────
    // SAFETY: `pid` came from a child we just spawned with Command::spawn().
    // Race: if the child already exited and the PID was reused we may signal
    // an unrelated process — this is an inherent POSIX limitation.  We check
    // the return value and log, but do not abort.
    let rc = unsafe { libc::kill(pid, signal) };
    if rc != 0 {
        warn!(
            "kill({}, {}) failed: {}",
            pid,
            signal,
            std::io::Error::last_os_error()
        );
    } else {
        info!("Sent signal {} to root PID {}", signal, pid);
    }

    // ── 2. Wait up to 5 s for the root to exit ──────────────────────────────
    // We poll with kill(pid, 0): returns 0 while alive, -1/ESRCH when gone.
    // (The child is reaped by `child.wait()` in the caller; here we only
    //  check liveness so we can decide whether to escalate.)
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        // SAFETY: same as above.
        if unsafe { libc::kill(pid, 0) } != 0 {
            info!("Root process {} exited.", pid);
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            warn!(
                "Root process {} did not exit in 5 s after signal {}.",
                pid, signal
            );
            break;
        }
    }

    // ── 3. Signal all descendant processes ──────────────────────────────────
    for &child_pid in &descendants {
        // SAFETY: same race caveat as above; we ignore ESRCH (already gone).
        let rc = unsafe { libc::kill(child_pid, signal) };
        if rc == 0 {
            info!("Sent signal {} to descendant PID {}", signal, child_pid);
        }
        // silently skip ESRCH — process may have already exited on its own
    }

    // ── 4. Wait up to 10 s for descendants; SIGKILL survivors ───────────────
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // SAFETY: kill(pid, 0) is a liveness probe, no signal is delivered.
        let alive: Vec<i32> = descendants
            .iter()
            .copied()
            .filter(|&p| unsafe { libc::kill(p, 0) } == 0)
            .collect();

        if alive.is_empty() {
            info!("All descendant processes exited.");
            break;
        }

        if tokio::time::Instant::now() >= deadline {
            warn!(
                "{} descendant(s) still alive after 10 s — sending SIGKILL",
                alive.len()
            );
            for &child_pid in &alive {
                warn!("Force-killing descendant PID {}", child_pid);
                // SAFETY: same rationale; SIGKILL cannot be caught or ignored.
                let _ = unsafe { libc::kill(child_pid, libc::SIGKILL) };
            }
            break;
        }
    }

    // ── 5. Ensure root is gone (escalate to SIGKILL if needed) ──────────────
    // SAFETY: liveness probe.
    if unsafe { libc::kill(pid, 0) } == 0 {
        warn!("Root process {} still alive — sending SIGKILL", pid);
        // SAFETY: same rationale as above.
        let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
    }

    info!("Process tree termination complete for PID {}.", pid);
}

/// A single line of log output from the executor
#[derive(Debug, Clone)]
pub struct LogLine {
    pub line: String,
    #[allow(dead_code)]
    pub is_stderr: bool,
}

/// Result of a job execution
#[derive(Debug)]
#[allow(dead_code)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub output: String,
}

/// Running job state for cancellation
pub struct RunningJob {
    /// Process ID (negative for process group)
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub async fn execute(
        &self,
        command: &str,
        work_dir: &str,
        environment: &HashMap<String, String>,
    ) -> anyhow::Result<ExecutionResult> {
        info!("Executing command: {}", command);

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(work_dir)
            .envs(environment)
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

    /// Mask common secret patterns in a string for safe logging.
    fn mask_secrets(s: &str) -> String {
        let patterns = [
            "password=",
            "token=",
            "secret=",
            "api_key=",
            "Bearer ",
            "Authorization:",
        ];
        let mut masked = s.to_string();
        for pat in &patterns {
            if let Some(idx) = masked.find(pat) {
                let start = idx + pat.len();
                let end = masked[start..]
                    .find(|c: char| c.is_whitespace() || c == '\'' || c == '"')
                    .map(|i| start + i)
                    .unwrap_or(masked.len());
                masked.replace_range(start..end, "***");
            }
        }
        masked
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
        secret_values: Arc<Vec<String>>,
        environment: &HashMap<String, String>,
    ) -> anyhow::Result<ExecutionResult> {
        info!(
            "Executing command (streaming): {}",
            Self::mask_secrets(command)
        );
        info!("Working directory: {}", work_dir);
        info!("Log path: {}", log_path);

        // Use process group(0) to create a new process group
        // This allows us to kill all child processes on cancel
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(work_dir)
            .envs(environment)
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
        let stdout_secrets = secret_values.clone();
        let stdout_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(raw_line)) = lines.next_line().await {
                if running_job_stdout.lock().await.cancelled {
                    break;
                }
                let line = mask_secret_values(&raw_line, &stdout_secrets);
                {
                    let mut f = stdout_log_file.lock().await;
                    if let Err(e) = f.write_all(line.as_bytes()).await {
                        tracing::warn!("Failed to write stdout log: {}", e);
                    }
                    if let Err(e) = f.write_all(b"\n").await {
                        tracing::warn!("Failed to write stdout newline: {}", e);
                    }
                    if let Err(e) = f.flush().await {
                        tracing::warn!("Failed to flush stdout log: {}", e);
                    }
                }
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
        let stderr_secrets = secret_values.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(raw_line)) = lines.next_line().await {
                if running_job_stderr.lock().await.cancelled {
                    break;
                }
                let line = mask_secret_values(&raw_line, &stderr_secrets);
                {
                    let mut f = stderr_log_file.lock().await;
                    if let Err(e) = f.write_all(line.as_bytes()).await {
                        tracing::warn!("Failed to write stderr log: {}", e);
                    }
                    if let Err(e) = f.write_all(b"\n").await {
                        tracing::warn!("Failed to write stderr newline: {}", e);
                    }
                    if let Err(e) = f.flush().await {
                        tracing::warn!("Failed to flush stderr log: {}", e);
                    }
                }
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

            // Cancellation signal — receives the signal number to send (e.g. SIGINT=2)
            Some(signal) = cancel_rx.recv() => {
                info!(
                    "Received cancel signal for job, terminating process tree (root PID: {})",
                    pid
                );

                // Set the cancelled flag so the reader tasks break out of their
                // next_line() loop on the next iteration check.
                running_job.lock().await.cancelled = true;

                // Walk the entire process tree and terminate every process,
                // including grandchildren that created their own process groups
                // (e.g. nix build sandboxes). Escalates to SIGKILL automatically
                // if any process does not exit within its grace period.
                terminate_process_tree(pid, signal).await;

                // Reap the direct child (`sh`).  This MUST be called here because
                // only the parent process may waitpid() on a child; terminate_process_tree
                // can only probe liveness via kill(pid, 0) for the root.
                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(5),
                    child.wait(),
                )
                .await
                {
                    Ok(Ok(status)) => info!("Direct child reaped: {:?}", status),
                    Ok(Err(e))     => warn!("Error reaping direct child: {}", e),
                    Err(_)         => error!("Timed out waiting for direct child to be reaped"),
                }

                // Abort the stdout/stderr reader tasks.
                //
                // Descendants that survived in their own process groups keep the
                // pipe write-ends open, so `lines.next_line().await` blocks forever.
                // Aborting the tasks:
                //   1. Cancels the blocked next_line() call immediately.
                //   2. Drops the `log_tx` sender clones held by each task.
                //   3. Closes the channel → log streamer finishes at once.
                info!("Aborting pipe reader tasks to unblock log streamer");
                stdout_handle.abort();
                stderr_handle.abort();

                // Return negative of the signal to indicate cancellation
                -signal
            }
        };

        // Wait for both readers to finish.
        // Normal exit path: give up to 2s for the pipe to drain naturally.
        // Cancellation path: tasks were aborted above so these resolve instantly
        // (with JoinError::Cancelled, which we intentionally ignore).
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), stdout_handle).await;
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), stderr_handle).await;

        Ok(ExecutionResult {
            exit_code,
            output: String::new(), // output is in the stream, not buffered
        })
    }
}

/// Replace all occurrences of each secret value with `***`.
pub fn mask_secret_values(s: &str, secrets: &[String]) -> String {
    let mut masked = s.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            masked = masked.replace(secret.as_str(), "***");
        }
    }
    masked
}
