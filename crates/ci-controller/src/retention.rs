//! Three-tier retention loop (issue #20, T5a/T8).
//!
//! Replaces the single-rule `max_age_days` task. On every tick the loop
//! re-resolves t1/t2/t3/max_builds_per_repo via
//! `state.resolve_setting_u64()` so Settings PUTs take effect on the
//! next tick without a controller restart.
//!
//! Tier semantics:
//! - T1 deletes controller-side per-group scratch dirs and (when fanout
//!   is enabled) pushes purge directives to owning workers.
//! - T2 archives terminal groups via `storage::archive_groups_batch`.
//! - T3 hard-deletes from `*_archive` via `storage::delete_archive_batch`.
//! - `max_builds_per_repo` runs after T2 and pushes surplus groups
//!   through T2 (archive, not delete).
//!
//! `run_once` is the core sweep logic, called by both the periodic loop
//! and the `ForceRetentionTick` admin RPC.
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::state::ControllerState;
use ci_core::models::config::RetentionConfig;

/// Options for a single retention sweep (used by `run_once`).
pub struct RunOnceOpts {
    /// Run T1 (file purge). If all three are false, all tiers run.
    pub run_t1: bool,
    /// Run T2 (archive). If all three are false, all tiers run.
    pub run_t2: bool,
    /// Run T3 (delete archive). If all three are false, all tiers run.
    pub run_t3: bool,
}

impl RunOnceOpts {
    /// Run all three tiers.
    pub fn all() -> Self {
        Self {
            run_t1: true,
            run_t2: true,
            run_t3: true,
        }
    }
}

/// Counts returned by a single retention sweep.
pub struct RunOnceResult {
    pub t1_purged: i64,
    pub t2_archived: i64,
    pub t3_deleted: i64,
}

/// Spawn the retention background loop.
///
/// `fallback` is the YAML default; per-tick values come from
/// `state.resolve_setting_u64()`.
pub fn spawn_cleanup_task(
    state: Arc<ControllerState>,
    fallback: RetentionConfig,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Read interval once at startup. Runtime changes are detected and
        // logged; the actual interval is fixed until controller restart.
        let initial_interval_secs = state
            .resolve_setting_u64(
                "retention.cleanup_interval_secs",
                fallback.cleanup_interval_secs,
            )
            .await
            .max(60);
        let mut interval = tokio::time::interval(Duration::from_secs(initial_interval_secs));
        interval.tick().await; // skip first immediate tick

        info!(
            interval_secs = initial_interval_secs,
            t1_default = fallback.t1_purge_files_after_days,
            t2_default = fallback.t2_archive_after_days,
            t3_default = fallback.t3_delete_archive_after_days,
            max_per_repo_default = fallback.max_builds_per_repo,
            "Retention cleanup started (three-tier)"
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Retention cleanup shutting down");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = tick_once(&state, &fallback, initial_interval_secs).await {
                        warn!(error = %e, "Retention cleanup tick failed");
                    }
                }
            }
        }
    })
}

/// Single-tick wrapper: re-validates settings and calls `run_once(all)`.
async fn tick_once(
    state: &Arc<ControllerState>,
    fallback: &RetentionConfig,
    initial_interval_secs: u64,
) -> anyhow::Result<()> {
    // Validate tier ordering before running anything.
    let t1 = state
        .resolve_setting_u64(
            "retention.t1_purge_files_after_days",
            fallback.t1_purge_files_after_days as u64,
        )
        .await as i32;
    let t2 = state
        .resolve_setting_u64(
            "retention.t2_archive_after_days",
            fallback.t2_archive_after_days as u64,
        )
        .await as i32;
    let t3 = state
        .resolve_setting_u64(
            "retention.t3_delete_archive_after_days",
            fallback.t3_delete_archive_after_days as u64,
        )
        .await as i32;

    let live_interval_secs = state
        .resolve_setting_u64(
            "retention.cleanup_interval_secs",
            fallback.cleanup_interval_secs,
        )
        .await
        .max(60);
    if live_interval_secs != initial_interval_secs {
        warn!(
            startup = initial_interval_secs,
            current = live_interval_secs,
            "retention.cleanup_interval_secs changed; restart required to take effect"
        );
    }

    if t1 <= 0 || t2 <= 0 || t3 <= 0 {
        warn!(t1, t2, t3, "retention tiers must be > 0; skipping tick");
        return Ok(());
    }
    if t1 >= t2 || t2 >= t3 {
        warn!(
            t1,
            t2, t3, "retention tier ordering invalid (need t1 < t2 < t3); skipping tick"
        );
        return Ok(());
    }

    run_once(state, fallback, RunOnceOpts::all()).await?;
    Ok(())
}

/// Run one retention sweep with the given opts.
///
/// This is the core logic used by both the periodic loop (via `tick_once`)
/// and the `ForceRetentionTick` admin RPC. The `fallback` is the YAML
/// default; per-call values are re-resolved from the DB via
/// `state.resolve_setting_u64()`.
///
/// When all three `run_*` fields are false, all three tiers are run
/// (equivalent to passing `RunOnceOpts::all()`).
pub async fn run_once(
    state: &Arc<ControllerState>,
    fallback: &RetentionConfig,
    opts: RunOnceOpts,
) -> anyhow::Result<RunOnceResult> {
    let storage = state
        .storage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No storage"))?;

    let run_all = !opts.run_t1 && !opts.run_t2 && !opts.run_t3;

    let t1 = state
        .resolve_setting_u64(
            "retention.t1_purge_files_after_days",
            fallback.t1_purge_files_after_days as u64,
        )
        .await as i32;
    let t2 = state
        .resolve_setting_u64(
            "retention.t2_archive_after_days",
            fallback.t2_archive_after_days as u64,
        )
        .await as i32;
    let t3 = state
        .resolve_setting_u64(
            "retention.t3_delete_archive_after_days",
            fallback.t3_delete_archive_after_days as u64,
        )
        .await as i32;
    let max_per_repo = state
        .resolve_setting_u64(
            "retention.max_builds_per_repo",
            fallback.max_builds_per_repo as u64,
        )
        .await as i32;
    let enable_worker_fanout = state
        .resolve_setting_bool(
            "retention.enable_worker_fanout",
            fallback.enable_worker_fanout,
        )
        .await;

    let mut result = RunOnceResult {
        t1_purged: 0,
        t2_archived: 0,
        t3_deleted: 0,
    };

    if opts.run_t1 || run_all {
        result.t1_purged = run_t1(state, storage, t1, enable_worker_fanout).await? as i64;
    }
    if opts.run_t2 || run_all {
        result.t2_archived = run_t2(storage, t2, max_per_repo).await? as i64;
    }
    if opts.run_t3 || run_all {
        result.t3_deleted = run_t3(storage, t3).await? as i64;
    }

    Ok(result)
}

/// T1 — delete controller-side per-group scratch dirs for terminal
/// groups whose `completed_at` is older than `t1_days`.
///
/// When `enable_worker_fanout=false`, stamps `files_purged_at` immediately.
/// When `enable_worker_fanout=true`, enqueues per-worker Redis directives
/// and stamps only for groups with no live workers.
///
/// Returns the number of groups stamped.
async fn run_t1(
    state: &ControllerState,
    storage: &crate::storage::Storage,
    t1_days: i32,
    enable_worker_fanout: bool,
) -> anyhow::Result<u64> {
    let candidates = storage.find_groups_for_t1(t1_days, 100).await?;
    if candidates.is_empty() {
        return Ok(0);
    }

    // Delete controller-side scratch dirs (best-effort).
    if let Some(log_dir) = &state.config.logging.log_dir {
        for gid in &candidates {
            let path = std::path::Path::new(log_dir).join(gid.to_string());
            match tokio::fs::remove_dir_all(&path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => warn!(
                    group_id = %gid,
                    path = %path.display(),
                    error = %e,
                    "T1: failed to remove controller scratch dir"
                ),
            }
        }
    }
    info!(count = candidates.len(), "T1: purged controller-side files");

    if !enable_worker_fanout {
        let stamped = storage
            .mark_files_purged(&candidates, chrono::Utc::now())
            .await?;
        if stamped > 0 {
            info!(count = stamped, "T1: stamped files_purged_at");
        }
        return Ok(stamped);
    }

    // Fanout path: enqueue per-worker directives via Redis.
    let redis = match &state.redis_store {
        Some(r) => r.clone(),
        None => {
            warn!(
                "T1: enable_worker_fanout=true but no redis_store; \
                 falling back to immediate stamp"
            );
            let stamped = storage
                .mark_files_purged(&candidates, chrono::Utc::now())
                .await?;
            if stamped > 0 {
                info!(count = stamped, "T1: stamped files_purged_at (no redis)");
            }
            return Ok(stamped);
        }
    };

    let heartbeat_timeout_secs = state.config.workers.heartbeat_timeout_secs as i64;
    let mut stamp_immediately: Vec<uuid::Uuid> = Vec::new();

    for gid in &candidates {
        let owners = storage.workers_for_group(*gid).await.unwrap_or_default();
        let live = filter_live_workers(state, &owners, heartbeat_timeout_secs).await;

        if live.is_empty() {
            stamp_immediately.push(*gid);
            continue;
        }

        for wid in &live {
            if let Err(e) = redis.enqueue_purge(wid, &gid.to_string()).await {
                warn!(
                    worker_id = %wid,
                    group_id = %gid,
                    error = %e,
                    "T1: failed to enqueue purge; will retry next tick"
                );
            }
        }
    }

    let mut stamped = 0u64;
    if !stamp_immediately.is_empty() {
        stamped = storage
            .mark_files_purged(&stamp_immediately, chrono::Utc::now())
            .await?;
        if stamped > 0 {
            info!(
                count = stamped,
                "T1: stamped files_purged_at for groups with no live workers"
            );
        }
    }
    Ok(stamped)
}

/// Return the subset of `owners` whose last heartbeat is within
/// `heartbeat_timeout_secs`. Workers absent from the registry or past the
/// timeout are treated as unreachable.
async fn filter_live_workers(
    state: &ControllerState,
    owners: &[String],
    heartbeat_timeout_secs: i64,
) -> Vec<String> {
    let registry = state.worker_registry.read().await;
    let now = chrono::Utc::now();
    owners
        .iter()
        .filter(|wid| {
            registry
                .get(wid)
                .and_then(|w| w.last_heartbeat.as_ref())
                .map(|hb| (now - hb.timestamp).num_seconds() < heartbeat_timeout_secs)
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// T2 — archive terminal groups older than `t2_days` and enforce per-repo cap.
/// Returns total groups archived.
async fn run_t2(
    storage: &crate::storage::Storage,
    t2_days: i32,
    max_per_repo: i32,
) -> anyhow::Result<u64> {
    let candidates = storage.find_groups_for_t2(t2_days, 100).await?;
    let mut total_archived = 0u64;
    if !candidates.is_empty() {
        total_archived += storage.archive_groups_batch(&candidates).await?;
        info!(count = total_archived, "T2: archived");
    }

    if max_per_repo > 0 {
        let excess = storage.find_excess_groups_per_repo(max_per_repo).await?;
        if !excess.is_empty() {
            for batch in excess.chunks(100) {
                let archived = storage.archive_groups_batch(batch).await?;
                if archived > 0 {
                    total_archived += archived;
                    info!(count = archived, "max_builds_per_repo: archived surplus");
                }
                tokio::task::yield_now().await;
            }
        }
    }
    Ok(total_archived)
}

/// T3 — hard-delete archive rows whose `archived_at` is older than `t3_days`.
/// Returns groups deleted.
async fn run_t3(storage: &crate::storage::Storage, t3_days: i32) -> anyhow::Result<u64> {
    let candidates = storage.find_archive_for_t3(t3_days, 100).await?;
    if candidates.is_empty() {
        return Ok(0);
    }
    let deleted = storage.delete_archive_batch(&candidates).await?;
    info!(count = deleted, "T3: hard-deleted archive");
    Ok(deleted)
}
