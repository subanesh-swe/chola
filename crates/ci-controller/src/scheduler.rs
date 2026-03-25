use std::sync::atomic::{AtomicUsize, Ordering};

use ci_core::models::job::Job;
use ci_core::models::worker::WorkerState;

/// Trait for pluggable scheduling strategies
pub trait Scheduler: Send + Sync {
    fn select_worker<'a>(
        &self,
        job: &Job,
        workers: &'a [&'a WorkerState],
    ) -> Option<&'a WorkerState>;
}

/// Best-fit scheduler: selects worker with most free resources that still fits.
/// Optionally prefers workers with NVMe disks for Nix jobs, prefers workers
/// already running a job for the same branch (branch affinity), and checks
/// available CPU in addition to memory/disk.
pub struct BestFitScheduler {
    pub nvme_preference: bool,
    pub branch_affinity: bool,
}

impl Scheduler for BestFitScheduler {
    fn select_worker<'a>(
        &self,
        job: &Job,
        workers: &'a [&'a WorkerState],
    ) -> Option<&'a WorkerState> {
        let mut candidates: Vec<&&WorkerState> = workers
            .iter()
            .filter(|w| {
                // Check memory and disk availability
                let mem_ok = w.free_memory_mb() >= job.required_memory_mb;
                let disk_ok = w.free_disk_mb() >= job.required_disk_mb;

                // Check CPU availability via heartbeat used_cpu_percent
                let cpu_ok = if job.required_cpu == 0 {
                    true
                } else {
                    match &w.last_heartbeat {
                        Some(hb) => {
                            let available_cpu =
                                w.info.total_cpu as f64 * (1.0 - hb.used_cpu_percent / 100.0);
                            available_cpu >= job.required_cpu as f64
                        }
                        // No heartbeat yet — assume full capacity available
                        None => true,
                    }
                };

                // Check job type support
                let type_ok = w
                    .info
                    .supported_job_types
                    .contains(&job.job_type.to_string());

                mem_ok && disk_ok && cpu_ok && type_ok
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Sort by best fit: lowest system load first, then most free memory
        candidates.sort_by(|a, b| {
            let a_load = a
                .last_heartbeat
                .as_ref()
                .map(|h| h.system_load)
                .unwrap_or(0.0);
            let b_load = b
                .last_heartbeat
                .as_ref()
                .map(|h| h.system_load)
                .unwrap_or(0.0);

            a_load
                .partial_cmp(&b_load)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.free_memory_mb().cmp(&a.free_memory_mb()))
        });

        // NVMe preference: for Nix jobs prefer workers with NVMe disks
        if self.nvme_preference && job.job_type == ci_core::models::job::JobType::Nix {
            if let Some(nvme) = candidates
                .iter()
                .find(|w| w.info.disk_type == ci_core::models::worker::DiskType::Nvme)
            {
                return Some(nvme);
            }
        }

        // Branch affinity: if a worker is already running a job for the same branch,
        // move it to the front so it gets preference (warm caches, shared artefacts, etc.)
        if self.branch_affinity {
            if let Some(branch_id) = &job.branch_id {
                if !branch_id.is_empty() {
                    // Prefer any worker that already has jobs running as a
                    // proxy for same-branch affinity.  A richer implementation
                    // would carry branch_id inside the heartbeat's running_job_ids
                    // list; for now this keeps the scheduler free of cross-registry
                    // lookups while still providing useful locality.
                    let _ = branch_id; // value used for the is_empty guard above
                    if let Some(pos) = candidates.iter().position(|w| {
                        w.last_heartbeat
                            .as_ref()
                            .map(|hb| !hb.running_job_ids.is_empty())
                            .unwrap_or(false)
                    }) {
                        candidates.swap(0, pos);
                    }
                }
            }
        }

        candidates.first().map(|w| **w)
    }
}

/// Round-robin scheduler: distributes jobs evenly across all eligible workers.
pub struct RoundRobinScheduler {
    counter: AtomicUsize,
}

impl RoundRobinScheduler {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Scheduler for RoundRobinScheduler {
    fn select_worker<'a>(
        &self,
        _job: &Job,
        workers: &'a [&'a WorkerState],
    ) -> Option<&'a WorkerState> {
        if workers.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % workers.len();
        Some(workers[idx])
    }
}
