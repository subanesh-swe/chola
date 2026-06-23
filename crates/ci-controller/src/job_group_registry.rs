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
        self.group_jobs.entry(group.id).or_default();
        self.groups.insert(group.id, group);
    }

    pub fn get(&self, group_id: &Uuid) -> Option<&JobGroup> {
        self.groups.get(group_id)
    }

    pub fn get_mut(&mut self, group_id: &Uuid) -> Option<&mut JobGroup> {
        self.groups.get_mut(group_id)
    }

    /// Bump `last_activity_at` to now for reaper timeout tracking.
    pub fn touch_activity(&mut self, group_id: &Uuid) {
        if let Some(g) = self.groups.get_mut(group_id) {
            g.last_activity_at = chrono::Utc::now();
        }
    }

    pub fn update_state(&mut self, group_id: &Uuid, new_state: JobGroupState) -> bool {
        if let Some(group) = self.groups.get_mut(group_id) {
            let valid = match (&group.state, &new_state) {
                // Can always cancel, fail, or expire
                (_, JobGroupState::Cancelled) => true,
                (_, JobGroupState::Failed) => true,
                (_, JobGroupState::Expired) => true,
                (JobGroupState::Pending, JobGroupState::Reserved) => true,
                (JobGroupState::Reserved, JobGroupState::Running) => true,
                (JobGroupState::Running, JobGroupState::Success) => true,
                _ => false,
            };
            if valid {
                info!(
                    "Job group {} state: {} -> {}",
                    group_id, group.state, new_state
                );
                group.state = new_state;
                group.updated_at = chrono::Utc::now();
                if new_state.is_terminal() {
                    group.completed_at = Some(chrono::Utc::now());
                }
                true
            } else {
                warn!(
                    "Invalid group state transition: {:?} -> {:?} for {}",
                    group.state, new_state, group_id
                );
                false
            }
        } else {
            false
        }
    }

    pub fn add_job_to_group(&mut self, group_id: &Uuid, job: Job) {
        info!("Adding job {} to group {}", job.job_id, group_id);
        self.group_jobs.entry(*group_id).or_default().push(job);
    }

    /// Mark `Running`/`Assigned` jobs in every group as `Unknown` — the
    /// sibling of `JobRegistry::mark_stale_jobs_unknown`, but for the
    /// per-group view used by `check_group_completion`. Called during
    /// startup recovery so the in-memory state on both sides agrees that
    /// jobs whose workers were mid-flight at crash time have an indeterminate
    /// outcome, instead of staying "running" forever. Returns the count of
    /// jobs that were transitioned.
    pub fn mark_stale_jobs_unknown(&mut self) -> usize {
        let now = chrono::Utc::now();
        let mut transitioned = 0usize;
        for jobs in self.group_jobs.values_mut() {
            for job in jobs.iter_mut() {
                if matches!(job.state, JobState::Running | JobState::Assigned) {
                    job.state = JobState::Unknown;
                    job.updated_at = now;
                    transitioned += 1;
                }
            }
        }
        transitioned
    }

    pub fn get_jobs_for_group(&self, group_id: &Uuid) -> &[Job] {
        self.group_jobs
            .get(group_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Update a job's state within a group
    pub fn update_job_in_group(
        &mut self,
        group_id: &Uuid,
        job_id: &str,
        state: JobState,
        exit_code: Option<i32>,
    ) {
        if let Some(jobs) = self.group_jobs.get_mut(group_id) {
            if let Some(job) = jobs.iter_mut().find(|j| job_id_matches(&j.job_id, job_id)) {
                job.state = state;
                job.exit_code = exit_code;
                job.updated_at = chrono::Utc::now();
                if state.is_terminal() {
                    job.completed_at = Some(chrono::Utc::now());
                    // Set status_reason on terminal transition
                    job.status_reason = match state {
                        JobState::Success => {
                            Some("Completed successfully (exit code 0)".to_string())
                        }
                        JobState::Failed => Some(format!(
                            "Command failed (exit code {})",
                            exit_code.unwrap_or(-1)
                        )),
                        JobState::Cancelled => Some("Cancelled".to_string()),
                        _ => None,
                    };
                }
            }
        }
    }

    pub fn get_job_mut_in_group(&mut self, group_id: &Uuid, job_id: &str) -> Option<&mut Job> {
        self.group_jobs
            .get_mut(group_id)?
            .iter_mut()
            .find(|j| job_id_matches(&j.job_id, job_id))
    }

    /// Check if a group has reached a terminal state.
    ///
    /// A group transitions to Success only when EVERY reserved stage has a
    /// submitted job AND all submitted jobs are terminal. Reserved stages
    /// that never get submitted keep the group in Running; the reservation
    /// reaper (workers.stall_timeout_secs) will eventually Expire it.
    ///
    /// Failure / cancellation propagate as soon as any submitted job hits
    /// that state — no need to wait for the rest in those cases.
    ///
    /// Sets `status_reason` on the group when it transitions to terminal.
    pub fn check_group_completion(&mut self, group_id: &Uuid) -> Option<JobGroupState> {
        let group = self.groups.get(group_id)?;
        if group.state.is_terminal() {
            return None;
        }
        let reserved_stages: Vec<String> = group.reserved_stages.clone();

        let jobs = self.group_jobs.get(group_id)?;
        if jobs.is_empty() {
            return None;
        }

        // Fast-fail: any failed/cancelled job collapses the whole group.
        let any_failed = jobs.iter().any(|j| j.state == JobState::Failed);
        let any_cancelled = jobs.iter().any(|j| j.state == JobState::Cancelled);

        if any_failed || any_cancelled {
            let (new_state, reason) = if any_failed {
                let reason = jobs
                    .iter()
                    .find(|j| j.state == JobState::Failed)
                    .map(|j| {
                        let stage = j.stage_name.as_deref().unwrap_or("unknown");
                        let code = j.exit_code.unwrap_or(-1);
                        format!("Stage {stage} failed (exit code {code})")
                    })
                    .unwrap_or_else(|| "Stage failed".to_string());
                (JobGroupState::Failed, reason)
            } else {
                (
                    JobGroupState::Cancelled,
                    "Cancelled: stage was cancelled".to_string(),
                )
            };
            self.update_state(group_id, new_state);
            if let Some(group) = self.groups.get_mut(group_id) {
                group.status_reason = Some(reason);
            }
            return Some(new_state);
        }

        // Success path: ALL submitted jobs must be terminal AND every
        // reserved stage must have a submitted job.
        let all_terminal = jobs.iter().all(|j| {
            matches!(
                j.state,
                JobState::Success | JobState::Failed | JobState::Cancelled | JobState::Unknown
            )
        });
        if !all_terminal {
            return None;
        }

        if !reserved_stages.is_empty() {
            // Every reserved stage name needs at least one submitted job.
            // We don't enforce one-job-per-stage (retries can submit twice);
            // having ≥1 job per reserved name is enough to call it "ran."
            let submitted_stages: std::collections::HashSet<&str> = jobs
                .iter()
                .filter_map(|j| j.stage_name.as_deref())
                .collect();
            let missing: Vec<&str> = reserved_stages
                .iter()
                .map(|s| s.as_str())
                .filter(|s| !submitted_stages.contains(s))
                .collect();
            if !missing.is_empty() {
                // Submitted jobs are done; we're still waiting for the rest
                // of the reservation manifest. Stay in Running — the
                // stall_timeout reaper will Expire the group if no further
                // submissions land.
                return None;
            }
        }

        self.update_state(group_id, JobGroupState::Success);
        if let Some(group) = self.groups.get_mut(group_id) {
            group.status_reason = Some("All stages completed successfully".to_string());
        }
        Some(JobGroupState::Success)
    }

    /// Evict terminal groups older than `max_age`. Returns count evicted.
    pub fn evict_terminal(&mut self, max_age: std::time::Duration) -> usize {
        let cutoff = chrono::Utc::now() - chrono::Duration::from_std(max_age).unwrap_or_default();
        let to_remove: Vec<Uuid> = self
            .groups
            .iter()
            .filter(|(_, g)| g.state.is_terminal() && g.updated_at < cutoff)
            .map(|(id, _)| *id)
            .collect();
        for id in &to_remove {
            self.groups.remove(id);
            self.group_jobs.remove(id);
        }
        to_remove.len()
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
                    JobState::Success | JobState::Failed | JobState::Cancelled | JobState::Unknown
                ) {
                    info!(
                        "Failing job {} in group {} due to worker death: {}",
                        job.job_id, group_id, reason
                    );
                    job.state = JobState::Failed;
                    job.output = Some(reason.to_string());
                    job.status_reason = Some(reason.to_string());
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

    // ── DAG dependency check ──

    /// Returns true if all stages listed in `depends_on` for `stage_name` have
    /// reached `JobState::Success` within the given group.
    pub fn can_submit_stage(
        &self,
        group_id: &Uuid,
        _stage_name: &str,
        depends_on: &[String],
    ) -> bool {
        if depends_on.is_empty() {
            return true;
        }
        let jobs = match self.group_jobs.get(group_id) {
            Some(j) => j,
            None => return false,
        };
        depends_on.iter().all(|dep| {
            jobs.iter()
                .any(|j| j.stage_name.as_deref() == Some(dep) && j.state == JobState::Success)
        })
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

/// Match a stored job_id against a worker-reported lookup id.
///
/// `do_submit_stage` stores jobs in the registry keyed by the raw worker
/// `job_id` (typically `"{group_id}-{stage_name}"`). Recovery, by contrast,
/// reads the DB primary key into the registry — and that key is
/// `Uuid::v5(NAMESPACE_OID, raw_job_id)` (see `db_job_to_job` in `main.rs`
/// and the `Uuid::new_v5` call sites in `grpc_server.rs`). Worker
/// `report_status` always sends the raw form, so post-restart lookups would
/// silently miss the recovery-loaded entries.
///
/// Accept either form to keep the completion cascade working across both
/// normal and post-recovery code paths.
fn job_id_matches(stored: &str, lookup_raw: &str) -> bool {
    if stored == lookup_raw {
        return true;
    }
    let v5 = Uuid::new_v5(&Uuid::NAMESPACE_OID, lookup_raw.as_bytes()).to_string();
    stored == v5
}

#[cfg(test)]
mod tests {
    use super::*;
    use ci_core::models::job::JobType;

    #[test]
    fn job_id_matches_raw_form() {
        let raw = "acab57ba-a068-478b-b755-f9299f3c7c66-vira-ci";
        assert!(job_id_matches(raw, raw));
    }

    #[test]
    fn job_id_matches_v5_form() {
        // Recovery stored the v5-hashed form (db.id.to_string()); worker
        // reports the raw form — they must still match.
        let raw = "acab57ba-a068-478b-b755-f9299f3c7c66-vira-ci";
        let v5 = Uuid::new_v5(&Uuid::NAMESPACE_OID, raw.as_bytes()).to_string();
        assert_ne!(raw, v5);
        assert!(job_id_matches(&v5, raw));
    }

    #[test]
    fn job_id_matches_rejects_unrelated() {
        let raw = "acab57ba-a068-478b-b755-f9299f3c7c66-vira-ci";
        let other_raw = "deadbeef-dead-beef-dead-beefdeadbeef-other";
        assert!(!job_id_matches(other_raw, raw));
    }

    #[test]
    fn mark_stale_jobs_unknown_transitions_running_and_assigned() {
        let mut reg = JobGroupRegistry::new();
        let g1 = Uuid::new_v4();
        let g2 = Uuid::new_v4();

        let mut j_run = Job::new("j1".into(), "/bin/true".into(), JobType::Common, 0, 0, 0);
        j_run.state = JobState::Running;
        let mut j_assigned = Job::new("j2".into(), "/bin/true".into(), JobType::Common, 0, 0, 0);
        j_assigned.state = JobState::Assigned;
        let mut j_done = Job::new("j3".into(), "/bin/true".into(), JobType::Common, 0, 0, 0);
        j_done.state = JobState::Success;

        reg.add_job_to_group(&g1, j_run);
        reg.add_job_to_group(&g1, j_done);
        reg.add_job_to_group(&g2, j_assigned);

        let transitioned = reg.mark_stale_jobs_unknown();
        assert_eq!(transitioned, 2);

        let g1_jobs = reg.get_jobs_for_group(&g1);
        assert_eq!(g1_jobs[0].state, JobState::Unknown);
        // Terminal jobs are untouched.
        assert_eq!(g1_jobs[1].state, JobState::Success);

        let g2_jobs = reg.get_jobs_for_group(&g2);
        assert_eq!(g2_jobs[0].state, JobState::Unknown);
    }

    #[test]
    fn update_job_in_group_matches_v5_stored_id() {
        // Simulates post-restart state: the registry holds a Job keyed by
        // the v5-hashed UUID (as recovery's `db_job_to_job` produces), but
        // the worker pushes status using the raw job_id.
        let group_id = Uuid::new_v4();
        let raw = format!("{group_id}-vira-ci");
        let v5 = Uuid::new_v5(&Uuid::NAMESPACE_OID, raw.as_bytes()).to_string();

        let mut reg = JobGroupRegistry::new();
        let mut job = Job::new(v5.clone(), "/bin/true".into(), JobType::Common, 0, 0, 0);
        job.state = JobState::Running;
        reg.add_job_to_group(&group_id, job);

        reg.update_job_in_group(&group_id, &raw, JobState::Success, Some(0));

        let stored = reg.get_jobs_for_group(&group_id);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].state, JobState::Success);
        assert_eq!(stored[0].exit_code, Some(0));
    }
}
