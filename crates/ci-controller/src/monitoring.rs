use std::sync::{
    atomic::{AtomicI64, AtomicU64, Ordering},
    Arc,
};

/// Shared Prometheus-compatible metrics for the controller.
///
/// All fields are cheaply cloneable `Arc<Atomic*>` so they can be passed to
/// both the gRPC server and the HTTP sidecar without locking.
#[derive(Clone)]
pub struct Metrics {
    // Counters (monotonically increasing)
    pub jobs_submitted: Arc<AtomicU64>,
    pub jobs_completed: Arc<AtomicU64>,
    pub jobs_failed: Arc<AtomicU64>,
    pub jobs_cancelled: Arc<AtomicU64>,
    pub stages_submitted: Arc<AtomicU64>,
    pub worker_reservations: Arc<AtomicU64>,
    pub stage_failures: Arc<AtomicU64>,

    // Gauges (can go up or down)
    pub active_builds: Arc<AtomicI64>,
    pub active_stages: Arc<AtomicI64>,
    pub connected_workers: Arc<AtomicI64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            jobs_submitted: Arc::new(AtomicU64::new(0)),
            jobs_completed: Arc::new(AtomicU64::new(0)),
            jobs_failed: Arc::new(AtomicU64::new(0)),
            jobs_cancelled: Arc::new(AtomicU64::new(0)),
            stages_submitted: Arc::new(AtomicU64::new(0)),
            worker_reservations: Arc::new(AtomicU64::new(0)),
            stage_failures: Arc::new(AtomicU64::new(0)),
            active_builds: Arc::new(AtomicI64::new(0)),
            active_stages: Arc::new(AtomicI64::new(0)),
            connected_workers: Arc::new(AtomicI64::new(0)),
        }
    }

    // ── Counter helpers ──────────────────────────────────────────────────────

    pub fn inc_jobs_submitted(&self) {
        self.jobs_submitted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_completed(&self) {
        self.jobs_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_failed(&self) {
        self.jobs_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_jobs_cancelled(&self) {
        self.jobs_cancelled.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_stages_submitted(&self) {
        self.stages_submitted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_worker_reservations(&self) {
        self.worker_reservations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_stage_failures(&self) {
        self.stage_failures.fetch_add(1, Ordering::Relaxed);
    }

    // ── Gauge helpers ─────────────────────────────────────────────────────────

    pub fn inc_active_builds(&self) {
        self.active_builds.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_active_builds(&self) {
        self.active_builds.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_active_stages(&self) {
        self.active_stages.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_active_stages(&self) {
        self.active_stages.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set_connected_workers(&self, count: i64) {
        self.connected_workers.store(count, Ordering::Relaxed);
    }

    // ── Prometheus text format ────────────────────────────────────────────────

    /// Render all metrics in Prometheus text exposition format (version 0.0.4).
    pub fn to_prometheus(&self) -> String {
        let jobs_submitted = self.jobs_submitted.load(Ordering::Relaxed);
        let jobs_completed = self.jobs_completed.load(Ordering::Relaxed);
        let jobs_failed = self.jobs_failed.load(Ordering::Relaxed);
        let jobs_cancelled = self.jobs_cancelled.load(Ordering::Relaxed);
        let stages_submitted = self.stages_submitted.load(Ordering::Relaxed);
        let worker_reservations = self.worker_reservations.load(Ordering::Relaxed);
        let stage_failures = self.stage_failures.load(Ordering::Relaxed);
        let active_builds = self.active_builds.load(Ordering::Relaxed);
        let active_stages = self.active_stages.load(Ordering::Relaxed);
        let connected_workers = self.connected_workers.load(Ordering::Relaxed);

        format!(
            "# HELP ci_jobs_submitted_total Total number of jobs submitted\n\
             # TYPE ci_jobs_submitted_total counter\n\
             ci_jobs_submitted_total {jobs_submitted}\n\
             # HELP ci_jobs_completed_total Total number of jobs completed successfully\n\
             # TYPE ci_jobs_completed_total counter\n\
             ci_jobs_completed_total {jobs_completed}\n\
             # HELP ci_jobs_failed_total Total number of jobs that failed\n\
             # TYPE ci_jobs_failed_total counter\n\
             ci_jobs_failed_total {jobs_failed}\n\
             # HELP ci_jobs_cancelled_total Total number of jobs cancelled\n\
             # TYPE ci_jobs_cancelled_total counter\n\
             ci_jobs_cancelled_total {jobs_cancelled}\n\
             # HELP ci_stages_submitted_total Total number of stages submitted\n\
             # TYPE ci_stages_submitted_total counter\n\
             ci_stages_submitted_total {stages_submitted}\n\
             # HELP ci_worker_reservation_count Total number of worker reservations made\n\
             # TYPE ci_worker_reservation_count counter\n\
             ci_worker_reservation_count {worker_reservations}\n\
             # HELP ci_stage_failures_total Total number of stage failures\n\
             # TYPE ci_stage_failures_total counter\n\
             ci_stage_failures_total {stage_failures}\n\
             # HELP ci_active_builds Current number of active (running) job groups\n\
             # TYPE ci_active_builds gauge\n\
             ci_active_builds {active_builds}\n\
             # HELP ci_active_stages Current number of active (running) stages\n\
             # TYPE ci_active_stages gauge\n\
             ci_active_stages {active_stages}\n\
             # HELP ci_connected_workers Current number of connected workers\n\
             # TYPE ci_connected_workers gauge\n\
             ci_connected_workers {connected_workers}\n\
             # HELP ci_up Controller process is running\n\
             # TYPE ci_up gauge\n\
             ci_up 1\n",
        )
    }
}

/// Legacy monitoring struct — kept for backwards compatibility with other modules
/// that may import it. Real metrics should use `Metrics`.
pub struct Monitoring;

impl Monitoring {
    pub fn new() -> Self {
        Self
    }

    /// Record worker resource metrics (legacy stub — use Metrics for real tracking)
    pub async fn record_worker_metrics(&self, worker_id: &str, load: f64) {
        tracing::info!("Worker {} load: {:.2}", worker_id, load);
    }

    /// Get job completion stats (legacy stub)
    pub async fn job_stats(&self) -> anyhow::Result<JobStats> {
        Ok(JobStats {
            queued: 0,
            running: 0,
            completed: 0,
            failed: 0,
        })
    }
}

#[derive(Debug)]
pub struct JobStats {
    pub queued: u64,
    pub running: u64,
    pub completed: u64,
    pub failed: u64,
}
