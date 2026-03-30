use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

const MAX_BUFFERED_BYTES_PER_JOB: usize = 50 * 1024 * 1024; // 50MB per job

/// A log chunk with metadata for broadcasting
#[derive(Clone, Debug)]
pub struct LogChunkData {
    pub worker_id: String,
    pub offset: u64,
    pub data: Vec<u8>,
    pub timestamp_unix: i64,
}

/// Tracks log state for a single job
struct JobLogState {
    /// Last byte offset received from the worker
    last_offset: u64,
    /// Total bytes received
    total_bytes: u64,
    /// Whether the job's log stream is complete
    complete: bool,
    /// Accumulated log data (stored in memory for now)
    log_data: Vec<u8>,
    /// Broadcast sender for live log subscribers
    /// Using broadcast channel allows multiple subscribers
    broadcast_tx: broadcast::Sender<LogChunkData>,
    /// When the log stream was finalized (for TTL-based cleanup)
    finalized_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Collects and stores logs from workers.
///
/// Receives LogChunk messages from the gRPC stream_logs handler,
/// tracks the byte offset per job for deduplication and resume,
/// and persists log data (currently to memory, later to storage).
///
/// Also supports real-time log streaming via broadcast channels.
pub struct LogAggregator {
    /// Per-job log tracking state
    jobs: HashMap<String, JobLogState>,
    /// Default channel capacity for broadcast channels
    broadcast_capacity: usize,
    /// Directory to write log files for Vector to tail
    log_dir: Option<String>,
}

impl LogAggregator {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            broadcast_capacity: 256, // Buffer up to 256 log chunks per job
            log_dir: None,
        }
    }

    pub fn with_log_dir(log_dir: String) -> Self {
        Self {
            jobs: HashMap::new(),
            broadcast_capacity: 256,
            log_dir: Some(log_dir),
        }
    }

    /// Create a new job log state with a broadcast channel
    fn new_job_state(&self) -> JobLogState {
        let (broadcast_tx, _) = broadcast::channel(self.broadcast_capacity);
        JobLogState {
            last_offset: 0,
            total_bytes: 0,
            complete: false,
            log_data: Vec::new(),
            broadcast_tx,
            finalized_at: None,
        }
    }

    /// Append a log chunk for a job. Returns the new last_offset.
    ///
    /// Handles:
    /// - Fresh job restart (offset == 0 on existing job resets state)
    /// - Out-of-order chunks (skips if offset < last_offset, i.e. duplicate)
    /// - Gap detection (warns if offset > last_offset, i.e. missed chunk)
    /// - Normal append (offset == last_offset)
    pub fn append_chunk(
        &mut self,
        job_id: &str,
        worker_id: &str,
        offset: u64,
        data: &[u8],
        timestamp_unix: i64,
    ) -> u64 {
        // Create new state outside the entry to avoid borrow conflict
        let broadcast_capacity = self.broadcast_capacity;
        let state = self.jobs.entry(job_id.to_string()).or_insert_with(|| {
            let (broadcast_tx, _) = broadcast::channel(broadcast_capacity);
            JobLogState {
                last_offset: 0,
                total_bytes: 0,
                complete: false,
                log_data: Vec::new(),
                broadcast_tx,
                finalized_at: None,
            }
        });

        // If offset is 0 and we have existing state, this is a fresh job run
        // (e.g., job ID was reused). Reset the state.
        if offset == 0 && state.last_offset > 0 {
            info!(
                "Fresh log stream for job {}: resetting (was at offset {})",
                job_id, state.last_offset
            );
            state.last_offset = 0;
            state.total_bytes = 0;
            state.complete = false;
            state.log_data.clear();
            state.finalized_at = None;
            // Create a new broadcast channel for the fresh job
            let (new_tx, _) = broadcast::channel(self.broadcast_capacity);
            state.broadcast_tx = new_tx;
        }

        if offset < state.last_offset {
            // Duplicate chunk — already received this data (e.g. after reconnect replay)
            info!(
                "Duplicate log chunk for job {}: offset {} < last_offset {}, skipping",
                job_id, offset, state.last_offset
            );
            return state.last_offset;
        }

        if offset > state.last_offset {
            // Gap detected — we missed some data
            warn!(
                "Log gap for job {}: expected offset {}, got {}. {} bytes missing.",
                job_id,
                state.last_offset,
                offset,
                offset - state.last_offset
            );
        }

        // Accept the chunk
        let chunk_len = data.len() as u64;
        state.last_offset = offset + chunk_len;
        state.total_bytes += chunk_len;

        // Enforce per-job memory bound
        if state.log_data.len() + data.len() > MAX_BUFFERED_BYTES_PER_JOB {
            if self.log_dir.is_some() {
                // Data is on disk — drain oldest bytes to make room
                let overflow = state.log_data.len() + data.len() - MAX_BUFFERED_BYTES_PER_JOB;
                let drain = overflow.min(state.log_data.len());
                state.log_data.drain(..drain);
                warn!(
                    "Log buffer overflow for job {}: drained {} bytes (disk-backed)",
                    job_id, drain
                );
            } else {
                // No disk — stop buffering; live broadcast still works
                warn!(
                    "Log buffer overflow for job {} (no disk): dropping {} bytes from buffer",
                    job_id,
                    data.len()
                );
                // Write to broadcast but skip in-memory append
                let chunk_data = LogChunkData {
                    worker_id: worker_id.to_string(),
                    offset,
                    data: data.to_vec(),
                    timestamp_unix,
                };
                let _ = state.broadcast_tx.send(chunk_data);
                return state.last_offset;
            }
        }

        // Store the log data
        state.log_data.extend_from_slice(data);

        // Write to disk for Vector to tail
        if let Some(ref log_dir) = self.log_dir {
            let log_path = format!("{}/{}.log", log_dir, job_id);
            // Use sync write since we're in a sync context (behind RwLock)
            if let Some(parent) = std::path::Path::new(&log_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let _ = file.write_all(data);
            }
        }

        // Broadcast to live subscribers (ignore errors if no subscribers)
        let chunk_data = LogChunkData {
            worker_id: worker_id.to_string(),
            offset,
            data: data.to_vec(),
            timestamp_unix,
        };
        let _ = state.broadcast_tx.send(chunk_data);

        debug!(
            "Log chunk: job={}, offset={}, bytes={}, total={}",
            job_id, offset, chunk_len, state.total_bytes
        );

        state.last_offset
    }

    /// Get the last acknowledged offset for a job.
    /// Used by the Reconnect RPC to tell the worker where to resume.
    pub fn last_offset(&self, job_id: &str) -> u64 {
        self.jobs.get(job_id).map(|s| s.last_offset).unwrap_or(0)
    }

    /// Mark a job's log stream as complete.
    pub fn finalize(&mut self, job_id: &str) {
        if let Some(state) = self.jobs.get_mut(job_id) {
            state.complete = true;
            state.finalized_at = Some(chrono::Utc::now());
            info!(
                "Log stream finalized for job {}: {} total bytes",
                job_id, state.total_bytes
            );
        }
    }

    /// Check if a job's log stream is complete.
    pub fn is_complete(&self, job_id: &str) -> bool {
        self.jobs.get(job_id).map(|s| s.complete).unwrap_or(false)
    }

    /// Get the accumulated log data for a job.
    /// Returns an empty slice if the job doesn't exist.
    pub fn get_log_data(&self, job_id: &str) -> &[u8] {
        self.jobs
            .get(job_id)
            .map(|s| s.log_data.as_slice())
            .unwrap_or(&[])
    }

    /// Get the log data as a string (for display purposes).
    pub fn get_log_string(&self, job_id: &str) -> String {
        String::from_utf8_lossy(self.get_log_data(job_id)).to_string()
    }

    /// Subscribe to live log updates for a job.
    ///
    /// Returns:
    /// - Buffered log data from `from_offset` onwards
    /// - A broadcast receiver for live updates
    /// - Whether the log stream is complete
    ///
    /// If the job doesn't exist yet, returns an empty buffer and a receiver
    /// that will receive chunks when the job starts.
    pub fn subscribe(
        &mut self,
        job_id: &str,
        from_offset: u64,
    ) -> (Vec<LogChunkData>, broadcast::Receiver<LogChunkData>, bool) {
        match self.jobs.get(job_id) {
            Some(state) => {
                // Clamp from_offset to actual buffer size
                let actual_offset = if from_offset as usize > state.log_data.len() {
                    warn!(
                        "Requested offset {} exceeds buffer size {}, clamping",
                        from_offset,
                        state.log_data.len()
                    );
                    state.log_data.len() as u64
                } else {
                    from_offset
                };

                // Get buffered data from the requested offset
                let buffered = if actual_offset == 0 {
                    // Return all data as a single chunk
                    if !state.log_data.is_empty() {
                        vec![LogChunkData {
                            worker_id: String::new(), // We don't track this for buffered data
                            offset: 0,
                            data: state.log_data.clone(),
                            timestamp_unix: 0,
                        }]
                    } else {
                        Vec::new()
                    }
                } else if actual_offset < state.log_data.len() as u64 {
                    // Return data from the offset onwards
                    let start = actual_offset as usize;
                    vec![LogChunkData {
                        worker_id: String::new(),
                        offset: actual_offset,
                        data: state.log_data[start..].to_vec(),
                        timestamp_unix: 0,
                    }]
                } else {
                    Vec::new()
                };

                let receiver = state.broadcast_tx.subscribe();
                (buffered, receiver, state.complete)
            }
            None => {
                // Job doesn't exist yet - create a placeholder state so subscribers can connect
                let (broadcast_tx, _) = broadcast::channel(self.broadcast_capacity);
                let receiver = broadcast_tx.subscribe();
                let state = JobLogState {
                    last_offset: 0,
                    total_bytes: 0,
                    complete: false,
                    log_data: Vec::new(),
                    broadcast_tx,
                    finalized_at: None,
                };
                self.jobs.insert(job_id.to_string(), state);
                (Vec::new(), receiver, false)
            }
        }
    }

    /// Clean up log buffers for finalized jobs older than the given age.
    pub fn cleanup_old_logs(&mut self, max_age_secs: i64) {
        let now = chrono::Utc::now();
        let to_remove: Vec<String> = self
            .jobs
            .iter()
            .filter(|(_, state)| {
                state.complete
                    && state
                        .finalized_at
                        .map_or(false, |t| (now - t).num_seconds() > max_age_secs)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &to_remove {
            self.jobs.remove(id);
        }
        if !to_remove.is_empty() {
            info!("Cleaned up {} completed job log buffers", to_remove.len());
        }
    }
}

impl Default for LogAggregator {
    fn default() -> Self {
        Self::new()
    }
}
