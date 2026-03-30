use std::collections::HashMap;
use tracing::info;

use ci_core::models::job::{Job, JobState};
use ci_core::proto::orchestrator::JobStatusUpdate;

/// In-memory job registry
pub struct JobRegistry {
    jobs: HashMap<String, Job>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
        }
    }

    pub fn add_job(&mut self, job: Job) {
        info!("Job added: {} ({})", job.job_id, job.job_type);
        self.jobs.insert(job.job_id.clone(), job);
    }

    pub fn get(&self, job_id: &str) -> Option<&Job> {
        self.jobs.get(job_id)
    }

    pub fn update_status(&mut self, update: &JobStatusUpdate) {
        if let Some(job) = self.jobs.get_mut(&update.job_id) {
            let new_state = match update.state {
                1 => JobState::Queued,
                2 => JobState::Assigned,
                3 => JobState::Running,
                4 => JobState::Success,
                5 => JobState::Failed,
                6 => JobState::Cancelled,
                _ => JobState::Unknown,
            };
            info!("Job {} state: {} -> {}", job.job_id, job.state, new_state);
            job.state = new_state;
            job.exit_code = Some(update.exit_code);
            if !update.output.is_empty() {
                job.output = Some(update.output.clone());
            }
            job.updated_at = chrono::Utc::now();
        }
    }

    /// Get next queued job for a given worker and mark it as assigned
    pub fn next_job_for(&mut self, worker_id: &str) -> Option<Job> {
        let job_id = self
            .jobs
            .values()
            .find(|j| j.state == JobState::Queued)
            .map(|j| j.job_id.clone())?;

        // Mark as assigned
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.state = JobState::Assigned;
            job.assigned_worker = Some(worker_id.to_string());
            job.updated_at = chrono::Utc::now();
            info!("Job {} assigned to worker {}", job_id, worker_id);
        }

        self.jobs.get(&job_id).cloned()
    }

    /// Assign a specific queued job to a worker. Returns the job if successful.
    pub fn assign_job(&mut self, job_id: &str, worker_id: &str) -> Option<Job> {
        if let Some(job) = self.jobs.get_mut(job_id) {
            if job.state == JobState::Queued {
                job.state = JobState::Assigned;
                job.assigned_worker = Some(worker_id.to_string());
                job.updated_at = chrono::Utc::now();
                info!("Job {} assigned to worker {}", job_id, worker_id);
                return Some(job.clone());
            }
        }
        None
    }

    pub fn queued_jobs(&self) -> Vec<&Job> {
        self.jobs
            .values()
            .filter(|j| j.state == JobState::Queued)
            .collect()
    }

    pub fn mark_unknown_for_worker(&mut self, worker_id: &str) -> usize {
        let mut count = 0;
        for job in self.jobs.values_mut() {
            if job.assigned_worker.as_deref() == Some(worker_id) && job.state == JobState::Running {
                info!(
                    "Job {} marked UNKNOWN (worker {} disconnected)",
                    job.job_id, worker_id
                );
                job.state = JobState::Unknown;
                job.updated_at = chrono::Utc::now();
                count += 1;
            }
        }
        count
    }

    /// Cancel a job. Returns the worker_id if the job was assigned to a worker.
    /// Note: This does NOT immediately set the state to CANCELLED - it just stores
    /// the cancel reason. The state will be set to CANCELLED when the worker reports
    /// the final status after terminating the process.
    pub fn cancel_job(&mut self, job_id: &str, reason: &str) -> Option<String> {
        if let Some(job) = self.jobs.get_mut(job_id) {
            match job.state {
                JobState::Success | JobState::Failed | JobState::Cancelled => {
                    info!(
                        "Job {} already in terminal state: {}",
                        job.job_id, job.state
                    );
                    return None;
                }
                _ => {
                    info!("Requesting cancellation for job {}: {}", job.job_id, reason);
                    // Don't set state to CANCELLED yet - wait for worker to report termination
                    job.cancel_reason = Some(reason.to_string());
                    job.updated_at = chrono::Utc::now();
                    return job.assigned_worker.clone();
                }
            }
        }
        None
    }

    /// Cancel orphaned jobs older than timeout_secs that have no job group
    pub fn cancel_orphaned_jobs(&mut self, timeout_secs: u64) -> usize {
        let now = chrono::Utc::now();
        let mut cancelled = 0;
        for job in self.jobs.values_mut() {
            if matches!(job.state, JobState::Queued | JobState::Assigned)
                && job.job_group_id.is_none()
                && (now - job.created_at).num_seconds() as u64 > timeout_secs
            {
                job.state = JobState::Cancelled;
                job.cancel_reason = Some("Orphaned (submitter disconnected)".to_string());
                job.updated_at = now;
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Get jobs that should be orphaned (submitter disconnected and timeout passed)
    pub fn get_orphaned_jobs(&self, connection_id: &str) -> Vec<&Job> {
        self.jobs
            .values()
            .filter(|j| {
                j.submitter_connection_id.as_deref() == Some(connection_id)
                    && matches!(
                        j.state,
                        JobState::Queued | JobState::Assigned | JobState::Running
                    )
            })
            .collect()
    }
}
