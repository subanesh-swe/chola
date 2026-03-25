use std::collections::HashMap;

use ci_core::models::job::{Job, JobState};
use ci_core::models::job_group::{JobGroup, JobGroupState};
use tracing::info;
#[allow(unused_imports)]
use tracing::warn;
use uuid::Uuid;

/// In-memory job group registry
pub struct JobGroupRegistry {
    groups: HashMap<Uuid, JobGroup>,
    /// Jobs within each group: group_id -> Vec<Job>
    group_jobs: HashMap<Uuid, Vec<Job>>,
}

impl JobGroupRegistry {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            group_jobs: HashMap::new(),
        }
    }

    pub fn add_group(&mut self, group: JobGroup) {
        info!("Job group added: {} (state: {})", group.id, group.state);
        self.group_jobs.entry(group.id).or_insert_with(Vec::new);
        self.groups.insert(group.id, group);
    }

    pub fn get(&self, group_id: &Uuid) -> Option<&JobGroup> {
        self.groups.get(group_id)
    }

    pub fn get_mut(&mut self, group_id: &Uuid) -> Option<&mut JobGroup> {
        self.groups.get_mut(group_id)
    }

    pub fn update_state(&mut self, group_id: &Uuid, state: JobGroupState) {
        if let Some(group) = self.groups.get_mut(group_id) {
            info!("Job group {} state: {} -> {}", group_id, group.state, state);
            group.state = state;
            group.updated_at = chrono::Utc::now();
            if state.is_terminal() {
                group.completed_at = Some(chrono::Utc::now());
            }
        }
    }

    pub fn add_job_to_group(&mut self, group_id: &Uuid, job: Job) {
        info!("Adding job {} to group {}", job.job_id, group_id);
        self.group_jobs
            .entry(*group_id)
            .or_insert_with(Vec::new)
            .push(job);
    }

    pub fn get_jobs_for_group(&self, group_id: &Uuid) -> &[Job] {
        self.group_jobs
            .get(group_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn get_job_mut_in_group(&mut self, group_id: &Uuid, job_id: &str) -> Option<&mut Job> {
        self.group_jobs
            .get_mut(group_id)?
            .iter_mut()
            .find(|j| j.job_id == job_id)
    }

    /// Check if all jobs in a group have reached terminal state
    pub fn check_group_completion(&mut self, group_id: &Uuid) -> Option<JobGroupState> {
        let jobs = self.group_jobs.get(group_id)?;
        if jobs.is_empty() {
            return None;
        }

        let all_terminal = jobs.iter().all(|j| {
            matches!(
                j.state,
                JobState::Success | JobState::Failed | JobState::Cancelled
            )
        });
        if !all_terminal {
            return None;
        }

        let any_failed = jobs.iter().any(|j| j.state == JobState::Failed);
        let any_cancelled = jobs.iter().any(|j| j.state == JobState::Cancelled);

        let new_state = if any_failed {
            JobGroupState::Failed
        } else if any_cancelled {
            JobGroupState::Cancelled
        } else {
            JobGroupState::Success
        };

        self.update_state(group_id, new_state);
        Some(new_state)
    }

    /// Return all groups that have not yet reached a terminal state.
    pub fn active_groups(&self) -> Vec<&JobGroup> {
        self.groups
            .values()
            .filter(|g| !g.state.is_terminal())
            .collect()
    }

    /// Get groups for a given worker
    pub fn get_groups_for_worker(&self, worker_id: &str) -> Vec<&JobGroup> {
        self.groups
            .values()
            .filter(|g| g.reserved_worker_id.as_deref() == Some(worker_id))
            .filter(|g| !g.state.is_terminal())
            .collect()
    }

    // ── Worker death / migration (5D) ──

    /// Handle worker death: find all active groups for this worker and classify them.
    ///
    /// Returns `(groups_to_migrate, groups_to_fail)`:
    /// - `groups_to_migrate`: groups that still have queued/assigned stages and may be
    ///   re-assigned to another worker.
    /// - `groups_to_fail`: groups where all remaining stages were already running
    ///   (now dead) and migration is not useful.
    pub fn handle_worker_death(&self, worker_id: &str) -> (Vec<Uuid>, Vec<Uuid>) {
        let mut to_migrate = Vec::new();
        let mut to_fail = Vec::new();

        let active_group_ids: Vec<Uuid> = self
            .groups
            .values()
            .filter(|g| g.reserved_worker_id.as_deref() == Some(worker_id))
            .filter(|g| !g.state.is_terminal())
            .map(|g| g.id)
            .collect();

        for group_id in active_group_ids {
            let has_pending = self
                .group_jobs
                .get(&group_id)
                .map(|jobs| {
                    jobs.iter()
                        .any(|j| matches!(j.state, JobState::Queued | JobState::Assigned))
                })
                .unwrap_or(false);

            if has_pending {
                // Group has stages that haven't started yet -- migration is possible.
                to_migrate.push(group_id);
            } else {
                // All stages were either completed or actively running (and now dead).
                to_fail.push(group_id);
            }
        }

        (to_migrate, to_fail)
    }

    /// Migrate a group to a new worker.
    ///
    /// Updates the group's `reserved_worker_id` and re-assigns any queued/assigned
    /// jobs to the new worker. Already-running or terminal jobs are left untouched.
    pub fn migrate_group(&mut self, group_id: &Uuid, new_worker_id: &str) {
        if let Some(group) = self.groups.get_mut(group_id) {
            info!(
                "Migrating group {} from {:?} to {}",
                group_id, group.reserved_worker_id, new_worker_id
            );
            group.reserved_worker_id = Some(new_worker_id.to_string());
            group.updated_at = chrono::Utc::now();
        }
        // Re-assign pending jobs to the new worker.
        if let Some(jobs) = self.group_jobs.get_mut(group_id) {
            for job in jobs.iter_mut() {
                if matches!(job.state, JobState::Queued | JobState::Assigned) {
                    job.assigned_worker = Some(new_worker_id.to_string());
                }
            }
        }
    }

    /// Mark all non-terminal jobs in a group as failed (worker died, no migration).
    ///
    /// Also transitions the group itself to `Failed`.
    pub fn fail_group_jobs(&mut self, group_id: &Uuid, reason: &str) {
        if let Some(jobs) = self.group_jobs.get_mut(group_id) {
            let now = chrono::Utc::now();
            for job in jobs.iter_mut() {
                if !matches!(
                    job.state,
                    JobState::Success | JobState::Failed | JobState::Cancelled
                ) {
                    info!(
                        "Failing job {} in group {} due to worker death: {}",
                        job.job_id, group_id, reason
                    );
                    job.state = JobState::Failed;
                    job.output = Some(reason.to_string());
                    job.updated_at = now;
                    job.completed_at = Some(now);
                }
            }
        }
        self.update_state(group_id, JobGroupState::Failed);
    }

    // ── Group completion / reservation release (5D) ──

    /// Called when a group reaches a terminal state.
    ///
    /// Returns the `worker_id` whose reservation should be released (if any).
    pub fn on_group_completed(&self, group_id: &Uuid) -> Option<String> {
        let group = self.groups.get(group_id)?;
        if !group.state.is_terminal() {
            return None;
        }
        group.reserved_worker_id.clone()
    }

    // ── Parallel stage execution (5E) ──

    /// Identify groups of stages that can execute in parallel.
    ///
    /// Returns a `Vec<Vec<String>>` where each inner vector contains job_ids that
    /// may run concurrently. Currently treats every queued job as an independent
    /// unit (parallel_group awareness requires stage_config data that lives in the
    /// database, not in-memory). Callers that want true parallel-group batching
    /// should look up `stage_configs.parallel_group` and merge accordingly.
    pub fn get_parallel_stages(&self, group_id: &Uuid) -> Vec<Vec<String>> {
        let jobs = match self.group_jobs.get(group_id) {
            Some(j) => j,
            None => return Vec::new(),
        };

        // Collect queued jobs. Without in-memory parallel_group metadata each job
        // is returned as its own batch of size 1.
        let mut result: Vec<Vec<String>> = Vec::new();
        for job in jobs {
            if job.state != JobState::Queued {
                continue;
            }
            result.push(vec![job.job_id.clone()]);
        }

        result
    }
}
