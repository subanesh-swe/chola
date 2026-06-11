//! Three-tier retention loop (issue #20, T5a).
//!
//! Replaces the single-rule `max_age_days` task. On every tick the loop
//! re-resolves t1/t2/t3/max_builds_per_repo via
//! `state.resolve_setting_u64()` so Settings PUTs take effect on the
//! next tick without a controller restart — mirrors the reservation
//! reaper pattern in `main.rs:495-540`.
//!
//! Tier semantics:
//! - T1 deletes controller-side per-group scratch dirs and (when fanout
//!   is enabled in T5b) pushes purge directives to owning workers.
//! - T2 archives terminal groups via `storage::archive_groups_batch`.
//! - T3 hard-deletes from `*_archive` via `storage::delete_archive_batch`.
//! - `max_builds_per_repo` runs after T3 and pushes surplus groups
//!   through T2 (archive, not delete) so the operator still has 1×
//!   `t3` window to recover.
//!
//! `cleanup_interval_secs` is read once at startup. Changing the value
//! mid-run logs a warning but does not retune the interval — that would
//! require restarting the `tokio::time::interval`.
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::state::ControllerState;
use ci_core::models::config::RetentionConfig;

/// Spawn the retention background loop.
///
/// `fallback` is the YAML default; per-tick values come from
/// `state.resolve_setting_u64()`. The `RetentionConfig` value is NOT
/// stored inside the spawned task — only `fallback` is captured by move
/// for use as the `resolve_setting_*` default argument.
pub fn spawn_cleanup_task(
    state: Arc<ControllerState>,
    fallback: RetentionConfig,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Read interval once at startup. Changing this value at runtime
        // is detected each tick and logged; the actual interval is fixed
        // until the next controller restart.
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
                    if let Err(e) = run_tick(&state, &fallback, initial_interval_secs).await {
                        warn!(error = %e, "Retention cleanup tick failed");
                    }
                }
            }
        }
    })
}

async fn run_tick(
    state: &ControllerState,
    fallback: &RetentionConfig,
    initial_interval_secs: u64,
) -> anyhow::Result<()> {
    let storage = state
        .storage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No storage"))?;

    // Re-resolve every knob from DB-with-fallback.
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

    // Detect interval drift: changing this value at runtime won't retune
    // the tokio interval — operators need to restart the controller.
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
            "retention.cleanup_interval_secs changed at runtime; restart required to take effect"
        );
    }

    // Validate tier ordering. Bail without touching anything if invalid.
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

    // Run T1 -> T2 -> T3 -> max-per-repo cap.
    run_t1(state, storage, t1, enable_worker_fanout).await?;
    run_t2(state, storage, t2).await?;
    run_t3(state, storage, t3).await?;
    run_max_per_repo(state, storage, max_per_repo).await?;

    Ok(())
}

/// T1 — delete controller-side per-group scratch dirs for terminal
/// groups whose `completed_at` is older than `t1_days`.
///
/// When `enable_worker_fanout=false` (the default until T5b lands) the
/// helper also stamps `files_purged_at` on the group so the controller
/// side is self-consistent. T5b changes this branch to enqueue per-worker
/// purge directives instead.
async fn run_t1(
    state: &ControllerState,
    storage: &crate::storage::Storage,
    t1_days: i32,
    enable_worker_fanout: bool,
) -> anyhow::Result<()> {
    let candidates = storage.find_groups_for_t1(t1_days, 100).await?;
    if candidates.is_empty() {
        return Ok(());
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
    info!(
        count = candidates.len(),
        "T1: purged controller-side files"
    );

    // Worker fanout lands in T5b. When disabled, stamp files_purged_at
    // immediately so the next tick doesn't re-find these groups.
    if !enable_worker_fanout {
        let stamped = storage
            .mark_files_purged(&candidates, chrono::Utc::now())
            .await?;
        if stamped > 0 {
            info!(count = stamped, "T1: stamped files_purged_at");
        }
    }
    // The `else` branch (fanout enabled) is intentionally a no-op here —
    // T5b adds the per-worker enqueue + deferred mark logic.

    Ok(())
}

/// T2 — archive terminal groups whose `completed_at` is older than `t2_days`.
async fn run_t2(
    _state: &ControllerState,
    storage: &crate::storage::Storage,
    t2_days: i32,
) -> anyhow::Result<()> {
    let candidates = storage.find_groups_for_t2(t2_days, 100).await?;
    if candidates.is_empty() {
        return Ok(());
    }
    let archived = storage.archive_groups_batch(&candidates).await?;
    info!(count = archived, "T2: archived");
    Ok(())
}

/// T3 — hard-delete archive rows whose `archived_at` is older than `t3_days`.
async fn run_t3(
    _state: &ControllerState,
    storage: &crate::storage::Storage,
    t3_days: i32,
) -> anyhow::Result<()> {
    let candidates = storage.find_archive_for_t3(t3_days, 100).await?;
    if candidates.is_empty() {
        return Ok(());
    }
    let deleted = storage.delete_archive_batch(&candidates).await?;
    info!(count = deleted, "T3: hard-deleted archive");
    Ok(())
}

/// `max_builds_per_repo` cap — push surplus groups through T2 (archive,
/// not delete). The archive copy still survives `t3` days, so operators
/// can recover before the row is gone for good.
async fn run_max_per_repo(
    _state: &ControllerState,
    storage: &crate::storage::Storage,
    max_per_repo: i32,
) -> anyhow::Result<()> {
    if max_per_repo <= 0 {
        return Ok(());
    }
    let excess = storage.find_excess_groups_per_repo(max_per_repo).await?;
    if excess.is_empty() {
        return Ok(());
    }
    for batch in excess.chunks(100) {
        let archived = storage.archive_groups_batch(batch).await?;
        if archived > 0 {
            info!(count = archived, "max_builds_per_repo: archived surplus");
        }
        tokio::task::yield_now().await;
    }
    Ok(())
}
