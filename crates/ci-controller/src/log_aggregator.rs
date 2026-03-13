use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{info, warn};

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
}

impl LogAggregator {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            broadcast_capacity: 256, // Buffer up to 256 log chunks per job
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

        // Store the log data
        state.log_data.extend_from_slice(data);

        // Broadcast to live subscribers (ignore errors if no subscribers)
        let chunk_data = LogChunkData {
            worker_id: worker_id.to_string(),
            offset,
            data: data.to_vec(),
            timestamp_unix,
        };
        let _ = state.broadcast_tx.send(chunk_data);

        info!(
            "Log chunk: job={}, offset={}, bytes={}, total={}",
            job_id, offset, chunk_len, state.total_bytes
        );

        // TODO: Persist to storage (S3/minio/local file)
        // For now, data is tracked but not stored persistently on the controller.
        // The worker's local log file serves as the durable copy.

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
                // Get buffered data from the requested offset
                let buffered = if from_offset == 0 {
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
                } else if from_offset < state.log_data.len() as u64 {
                    // Return data from the offset onwards
                    let start = from_offset as usize;
                    vec![LogChunkData {
                        worker_id: String::new(),
                        offset: from_offset,
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
                };
                self.jobs.insert(job_id.to_string(), state);
                (Vec::new(), receiver, false)
            }
        }
    }
}

impl Default for LogAggregator {
    fn default() -> Self {
        Self::new()
    }
}
