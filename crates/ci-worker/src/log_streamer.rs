use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use ci_core::proto::orchestrator::LogChunk;

use crate::executor::LogLine;
use crate::grpc_client::GrpcClient;

/// Configuration for log batching behavior
pub struct LogStreamerConfig {
    /// Max bytes to accumulate before flushing a batch
    pub batch_size_limit: usize,
    /// Max time to wait before flushing a partial batch
    pub batch_time_limit: Duration,
}

impl Default for LogStreamerConfig {
    fn default() -> Self {
        Self {
            batch_size_limit: 8 * 1024, // 8KB
            batch_time_limit: Duration::from_millis(500),
        }
    }
}

/// Streams log lines from the executor to the controller via gRPC.
///
/// Consumes lines from an mpsc channel, batches them by size or time,
/// tracks byte offset, and sends LogChunk messages over the StreamLogs RPC.
pub struct LogStreamer {
    config: LogStreamerConfig,
}

impl LogStreamer {
    pub fn new() -> Self {
        Self {
            config: LogStreamerConfig::default(),
        }
    }

    pub fn with_config(config: LogStreamerConfig) -> Self {
        Self { config }
    }

    /// Stream logs for a job from the executor channel to the controller.
    ///
    /// This method:
    /// 1. Receives LogLines from the executor via `log_rx`
    /// 2. Batches them by size (8KB) or time (500ms)
    /// 3. Sends each batch as a LogChunk to the controller via gRPC StreamLogs
    /// 4. Tracks the byte offset for resume capability
    ///
    /// Returns the final byte offset (total bytes sent).
    pub async fn stream(
        &self,
        worker_id: String,
        job_id: String,
        mut log_rx: mpsc::Receiver<LogLine>,
        client: GrpcClient,
    ) -> anyhow::Result<u64> {
        info!("Starting log stream for job {}", job_id);

        // Channel for sending LogChunks to the gRPC stream
        let (chunk_tx, chunk_rx) = mpsc::channel::<LogChunk>(64);

        // Spawn the gRPC streaming call in the background
        let grpc_job_id = job_id.clone();
        let grpc_handle = tokio::spawn(async move {
            match client.stream_logs(chunk_rx).await {
                Ok(ack) => {
                    info!(
                        "Log stream completed for job {}, controller acked offset {}",
                        grpc_job_id, ack.last_offset
                    );
                    Ok(ack.last_offset)
                }
                Err(e) => {
                    error!("Log stream gRPC error for job {}: {}", grpc_job_id, e);
                    Err(e)
                }
            }
        });

        // Batching loop: collect lines, flush by size or time
        let mut batch_buffer: Vec<u8> = Vec::new();
        let mut offset: u64 = 0;
        let mut flush_interval = tokio::time::interval(self.config.batch_time_limit);
        // Skip the first immediate tick
        flush_interval.tick().await;

        loop {
            tokio::select! {
                // Receive a line from the executor
                maybe_line = log_rx.recv() => {
                    match maybe_line {
                        Some(log_line) => {
                            // Append line + newline to batch buffer
                            batch_buffer.extend_from_slice(log_line.line.as_bytes());
                            batch_buffer.push(b'\n');

                            // Flush if batch is large enough
                            if batch_buffer.len() >= self.config.batch_size_limit {
                                let chunk = LogChunk {
                                    worker_id: worker_id.clone(),
                                    job_id: job_id.clone(),
                                    offset,
                                    data: batch_buffer.clone(),
                                    timestamp_unix: chrono::Utc::now().timestamp(),
                                };
                                offset += batch_buffer.len() as u64;
                                batch_buffer.clear();

                                if chunk_tx.send(chunk).await.is_err() {
                                    warn!("Log chunk channel closed, stopping stream for job {}", job_id);
                                    break;
                                }
                            }
                        }
                        None => {
                            // Channel closed — executor finished
                            // Flush remaining buffer
                            if !batch_buffer.is_empty() {
                                let chunk = LogChunk {
                                    worker_id: worker_id.clone(),
                                    job_id: job_id.clone(),
                                    offset,
                                    data: batch_buffer.clone(),
                                    timestamp_unix: chrono::Utc::now().timestamp(),
                                };
                                offset += batch_buffer.len() as u64;
                                batch_buffer.clear();

                                if chunk_tx.send(chunk).await.is_err() {
                                    warn!("Log chunk channel closed during final flush for job {}", job_id);
                                }
                            }
                            break;
                        }
                    }
                }

                // Timer fires — flush partial batch for low-latency
                _ = flush_interval.tick() => {
                    if !batch_buffer.is_empty() {
                        let chunk = LogChunk {
                            worker_id: worker_id.clone(),
                            job_id: job_id.clone(),
                            offset,
                            data: batch_buffer.clone(),
                            timestamp_unix: chrono::Utc::now().timestamp(),
                        };
                        offset += batch_buffer.len() as u64;
                        batch_buffer.clear();

                        if chunk_tx.send(chunk).await.is_err() {
                            warn!("Log chunk channel closed during timer flush for job {}", job_id);
                            break;
                        }
                    }
                }
            }
        }

        // Drop the chunk sender to close the gRPC stream
        drop(chunk_tx);

        // Wait for the gRPC call to complete and get the acked offset
        match grpc_handle.await {
            Ok(Ok(acked_offset)) => {
                info!(
                    "Log streaming finished for job {}: sent {} bytes, controller acked {} bytes",
                    job_id, offset, acked_offset
                );
                Ok(offset)
            }
            Ok(Err(e)) => {
                error!("Log streaming gRPC error for job {}: {}", job_id, e);
                // Return our local offset even if gRPC failed — logs are in local file
                Ok(offset)
            }
            Err(e) => {
                error!("Log streaming task panicked for job {}: {}", job_id, e);
                Ok(offset)
            }
        }
    }
}
