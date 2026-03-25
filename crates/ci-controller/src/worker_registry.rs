use std::collections::HashMap;
use tracing::info;

use ci_core::models::worker::{DiskType, WorkerHeartbeat, WorkerInfo, WorkerState, WorkerStatus};
use ci_core::proto::orchestrator::{HeartbeatMessage, RegisterRequest};

/// In-memory worker registry
pub struct WorkerRegistry {
    workers: HashMap<String, WorkerState>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self {
            workers: HashMap::new(),
        }
    }

    pub fn register(&mut self, req: &RegisterRequest) {
        let disk_type = match req.disk_type.as_str() {
            "nvme" => DiskType::Nvme,
            _ => DiskType::Sata,
        };

        let info = WorkerInfo {
            worker_id: req.worker_id.clone(),
            hostname: req.hostname.clone(),
            total_cpu: req.total_cpu,
            total_memory_mb: req.total_memory_mb,
            total_disk_mb: req.total_disk_mb,
            disk_type,
            supported_job_types: req.supported_job_types.clone(),
            docker_enabled: req.docker_enabled,
        };

        let state = WorkerState::new(info);
        self.workers.insert(req.worker_id.clone(), state);
        info!("Worker registered: {}", req.worker_id);
    }

    pub fn update_heartbeat(&mut self, msg: &HeartbeatMessage) {
        if let Some(worker) = self.workers.get_mut(&msg.worker_id) {
            worker.status = WorkerStatus::Connected;
            worker.last_heartbeat = Some(WorkerHeartbeat {
                worker_id: msg.worker_id.clone(),
                used_cpu_percent: msg.used_cpu_percent,
                used_memory_mb: msg.used_memory_mb,
                used_disk_mb: msg.used_disk_mb,
                running_job_ids: msg.running_job_ids.clone(),
                system_load: msg.system_load,
                timestamp: chrono::Utc::now(),
            });
        }
    }

    pub fn get(&self, worker_id: &str) -> Option<&WorkerState> {
        self.workers.get(worker_id)
    }

    pub fn connected_workers(&self) -> Vec<&WorkerState> {
        self.workers
            .values()
            .filter(|w| w.status == WorkerStatus::Connected)
            .collect()
    }

    pub fn mark_disconnected(&mut self, worker_id: &str) {
        if let Some(worker) = self.workers.get_mut(worker_id) {
            worker.status = WorkerStatus::Disconnected;
            info!("Worker marked disconnected: {}", worker_id);
        }
    }

    pub fn mark_reconnected(&mut self, worker_id: &str) {
        if let Some(worker) = self.workers.get_mut(worker_id) {
            worker.status = WorkerStatus::Connected;
            info!("Worker reconnected: {}", worker_id);
        } else {
            info!("Reconnected worker not in registry: {}", worker_id);
        }
    }

    /// Return a reference to every worker regardless of status.
    pub fn all_workers(&self) -> Vec<&WorkerState> {
        self.workers.values().collect()
    }

    /// Put a connected worker into drain mode so it stops receiving new jobs.
    ///
    /// Returns `true` if the worker was found and transitioned, `false` otherwise.
    pub fn mark_draining(&mut self, worker_id: &str) -> bool {
        if let Some(worker) = self.workers.get_mut(worker_id) {
            worker.status = WorkerStatus::Draining;
            info!("Worker {} entering drain mode", worker_id);
            true
        } else {
            false
        }
    }

    /// Returns `true` if the worker is currently in drain mode.
    pub fn is_draining(&self, worker_id: &str) -> bool {
        self.workers
            .get(worker_id)
            .map(|w| w.status == WorkerStatus::Draining)
            .unwrap_or(false)
    }

    /// Check for workers whose last heartbeat is older than `timeout_secs`.
    ///
    /// Marks timed-out workers as `Disconnected` and returns their IDs so the
    /// caller can trigger job-migration or failure flows.
    pub fn check_heartbeat_timeouts(&mut self, timeout_secs: u64) -> Vec<String> {
        let now = chrono::Utc::now();
        let mut timed_out = Vec::new();

        for (id, worker) in &self.workers {
            if worker.status == WorkerStatus::Connected {
                let last_hb = worker
                    .last_heartbeat
                    .as_ref()
                    .map(|hb| hb.timestamp)
                    .unwrap_or(worker.registered_at);
                let elapsed = (now - last_hb).num_seconds() as u64;
                if elapsed > timeout_secs {
                    timed_out.push(id.clone());
                }
            }
        }

        for id in &timed_out {
            if let Some(w) = self.workers.get_mut(id) {
                w.status = WorkerStatus::Disconnected;
                tracing::warn!(
                    "Worker {} heartbeat timeout ({}s elapsed)",
                    id,
                    timeout_secs
                );
            }
        }

        timed_out
    }
}
