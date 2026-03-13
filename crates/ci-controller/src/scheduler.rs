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

/// Best-fit scheduler: selects worker with most free resources that still fits
pub struct BestFitScheduler {
    pub nvme_preference: bool,
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
                // Check resource availability
                w.free_memory_mb() >= job.required_memory_mb
                    && w.free_disk_mb() >= job.required_disk_mb
                    // Check job type support
                    && w.info
                        .supported_job_types
                        .contains(&job.job_type.to_string())
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Sort by best fit: lowest load, most free memory
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

        // NVMe preference for nix jobs
        if self.nvme_preference && job.job_type == ci_core::models::job::JobType::Nix {
            if let Some(nvme) = candidates
                .iter()
                .find(|w| w.info.disk_type == ci_core::models::worker::DiskType::Nvme)
            {
                return Some(nvme);
            }
        }

        candidates.first().map(|w| **w)
    }
}
