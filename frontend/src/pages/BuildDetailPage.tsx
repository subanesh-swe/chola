import { useState, useEffect, type ReactNode } from 'react';
import { clsx } from 'clsx';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getBuild, cancelBuild, retryBuild, retryJob } from '../api/builds';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { PipelineExplorer } from '../components/pipeline/PipelineExplorer';
import { usePermission } from '../hooks/usePermission';
import { formatDuration } from '../utils/duration';
import { formatSecs, formatBytes } from '../utils/format';
import { PageSkeleton } from '../components/ui/PageSkeleton';
import { toast } from 'sonner';
import type { Job, JobGroup, ArchivedChildren, MutationError } from '../types';

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
  });
}

// ── Archived children panel ───────────────────────────────────────────────────

function ChildTable({ title, rows }: { title: string; rows: unknown[] }) {
  if (!rows.length) {
    return (
      <div className="mb-3">
        <p className="text-xs font-semibold text-slate-500 uppercase mb-1">{title}</p>
        <p className="text-xs text-slate-600 italic">(none)</p>
      </div>
    );
  }
  return (
    <div className="mb-3">
      <p className="text-xs font-semibold text-slate-500 uppercase mb-1">
        {title} ({rows.length})
      </p>
      <div className="overflow-x-auto">
        <pre className="text-[11px] text-slate-400 bg-slate-800/60 rounded p-2 overflow-x-auto max-h-40 whitespace-pre-wrap break-all">
          {JSON.stringify(rows, null, 2)}
        </pre>
      </div>
    </div>
  );
}

function ArchivedChildrenPanel({ children }: { children: ArchivedChildren }) {
  const [open, setOpen] = useState(false);

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl">
      <button
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center justify-between px-4 py-3 text-left hover:bg-slate-800/50 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 rounded-xl"
        aria-expanded={open}
      >
        <span className="text-sm font-semibold text-slate-400">Archived child records</span>
        <svg
          className={`w-4 h-4 text-slate-500 transition-transform ${open ? 'rotate-180' : ''}`}
          fill="none" stroke="currentColor" viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {open && (
        <div className="px-4 pb-4 border-t border-slate-800 pt-3">
          <ChildTable title="Artifacts" rows={children.artifacts ?? []} />
          <ChildTable title="Test results" rows={children.test_results ?? []} />
          <ChildTable title="Approval gates" rows={children.approval_gates ?? []} />
          <ChildTable title="Worker reservations" rows={children.worker_reservations ?? []} />
        </div>
      )}
    </div>
  );
}

// ── Job log panel ─────────────────────────────────────────────────────────────


interface TimerInfo {
  status: string;
  remaining_secs: number | null;
  max_secs: number;
  elapsed_secs?: number;
  stage_name?: string;
  reason?: string;
}

function TimerRow({ label, timer, job }: { label: string; timer: TimerInfo | undefined; job?: Job | null }) {
  const [now, setNow] = useState(Date.now());
  const status = timer?.status ?? 'na';
  const maxSecs = timer?.max_secs ?? 0;
  const reason = timer?.reason ?? (
    status === 'paused' ? 'Paused (stage running)' :
    status === 'deactivated' ? 'Deactivated' : '—'
  );

  const isLiveStage = status === 'active' && job?.started_at;
  useEffect(() => {
    if (!isLiveStage) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [isLiveStage]);

  const icon = status === 'active' ? '⏱' : status === 'paused' ? '⏸' : status === 'deactivated' ? '✓' : '○';
  const color = status === 'active' ? 'text-success' : status === 'paused' ? 'text-warning' : 'text-disabled';
  const maxLabel = maxSecs > 0 ? formatSecs(maxSecs) : 'no limit';

  let timeDisplay: string;
  if (status === 'active' && job?.started_at && maxSecs > 0) {
    const elapsed = Math.floor((now - new Date(job.started_at).getTime()) / 1000);
    const remaining = Math.max(0, maxSecs - elapsed);
    timeDisplay = `${formatSecs(remaining)} / ${maxLabel}`;
  } else if (status === 'active' && timer?.remaining_secs != null) {
    timeDisplay = `${formatSecs(Math.max(0, timer.remaining_secs))} / ${maxLabel}`;
  } else {
    timeDisplay = `— / ${maxLabel}`;
  }

  return (
    <div className="flex items-center justify-between text-xs">
      <div className="flex items-center gap-2">
        <span>{icon}</span>
        <span className="text-secondary">{label}</span>
      </div>
      <div className="flex items-center gap-3">
        <span className="text-secondary font-mono">{timeDisplay}</span>
        <span className={`${color} max-w-xs text-right truncate`} title={reason}>{reason}</span>
      </div>
    </div>
  );
}

/** Subheading + bordered group inside the Timers panel. */
function TimerGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="rounded-lg border border-border/60 bg-surface-2/30 p-3">
      <p className="text-[11px] font-semibold text-disabled uppercase tracking-wider mb-2">{title}</p>
      <div className="space-y-2">{children}</div>
    </div>
  );
}

/**
 * One row per stage in the Timers panel. Works for running AND terminal
 * stages:
 *  - running + timeout: live "X left / max" countdown, color escalates as
 *    the deadline nears.
 *  - running, no timeout: elapsed + "no limit".
 *  - terminal: final "used Y / max (pct)" — so people can see, after the
 *    fact, how much of each stage's budget was consumed.
 */
function StageTimerRow({ job }: { job: Job }) {
  const isRunning = job.state === 'running';
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!isRunning) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [isRunning]);

  const maxSecs = job.max_duration_secs || 0;
  const endMs = job.completed_at ? new Date(job.completed_at).getTime() : now;
  const elapsedSecs = job.started_at
    ? Math.max(0, Math.floor((endMs - new Date(job.started_at).getTime()) / 1000))
    : 0;
  const pct = maxSecs > 0 ? elapsedSecs / maxSecs : 0;

  // Icon + color by lifecycle.
  let icon: string;
  let color: string;
  if (isRunning) {
    icon = '⏱';
    color = pct > 0.9 ? 'text-danger' : pct > 0.7 ? 'text-warning' : 'text-success';
  } else if (job.state === 'success') {
    icon = '✓';
    color = 'text-disabled';
  } else if (job.state === 'failed') {
    icon = '✗';
    color = 'text-danger';
  } else {
    icon = '○';
    color = 'text-disabled';
  }

  const maxLabel = maxSecs > 0 ? formatSecs(maxSecs) : 'no limit';
  let timeDisplay: string;
  if (isRunning && maxSecs > 0) {
    timeDisplay = `${formatSecs(Math.max(0, maxSecs - elapsedSecs))} left / ${maxLabel}`;
  } else if (maxSecs > 0) {
    timeDisplay = `${formatSecs(elapsedSecs)} / ${maxLabel} (${Math.round(Math.min(pct, 1) * 100)}%)`;
  } else {
    timeDisplay = `${formatSecs(elapsedSecs)} / ${maxLabel}`;
  }

  return (
    <div className="flex items-center justify-between text-xs">
      <div className="flex items-center gap-2 min-w-0">
        <span aria-hidden="true">{icon}</span>
        <span className="text-secondary font-mono truncate">{job.stage_name}</span>
      </div>
      <span className={`${color} font-mono shrink-0`}>{timeDisplay}</span>
    </div>
  );
}

function TimersPanel({ group, jobs }: { group: JobGroup & { timers?: { idle?: TimerInfo; stall?: TimerInfo; stage?: TimerInfo } }; jobs: Job[] }) {
  const isTerminal = ['success', 'failed', 'cancelled', 'expired'].includes(group.state);

  // Show every stage that has a job (running or finished) so the panel is
  // useful both live and as a post-mortem.
  const sortedJobs = [...jobs].sort(
    (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime(),
  );
  const hasRunning = jobs.some(j => j.state === 'running' || j.state === 'assigned');

  // ── Build-level timers (stall + idle). Prefer server-computed values;
  // otherwise compute a sensible client-side view. ──
  const idleMax = group.timers?.idle?.max_secs ?? group.idle_timeout_secs ?? 300;
  const stallMax = group.timers?.stall?.max_secs ?? group.stall_timeout_secs ?? 1800;

  const stallTimer: TimerInfo = group.timers?.stall ?? (
    isTerminal
      ? { status: 'deactivated', remaining_secs: null, max_secs: stallMax, reason: 'Build finished' }
      : group.state === 'running'
        ? hasRunning
          ? { status: 'paused', remaining_secs: null, max_secs: stallMax, reason: 'Paused (stage running)' }
          : { status: 'active', remaining_secs: group.time_until_timeout_secs ?? stallMax, max_secs: stallMax, reason: 'Waiting for next stage' }
        : { status: 'na', remaining_secs: null, max_secs: stallMax }
  );

  const idleTimer: TimerInfo = group.timers?.idle ?? (
    isTerminal
      ? { status: 'deactivated', remaining_secs: null, max_secs: idleMax, reason: 'Build finished' }
      : group.state === 'reserved'
        ? { status: 'active', remaining_secs: group.time_until_timeout_secs ?? idleMax, max_secs: idleMax, reason: 'Waiting for first stage' }
        : { status: 'deactivated', remaining_secs: null, max_secs: idleMax }
  );

  const res = group.allocated_resources;
  const hasRes = res && (res.cpu > 0 || res.memory_mb > 0 || res.disk_mb > 0);

  return (
    <div className="bg-surface border border-border rounded-xl p-4">
      {/* Header row: card title on the left, compact reserved-resources
          strip (clearly labelled) on the right. */}
      <div className="flex items-center justify-between gap-3 mb-3 flex-wrap">
        <h3 className="text-xs font-semibold text-disabled uppercase tracking-wider">
          Timers &amp; Resources{isTerminal && <span className="ml-2 normal-case text-disabled/70 font-normal">— final</span>}
        </h3>
        {hasRes && (
          <div className="flex items-center gap-3 text-xs font-mono text-secondary">
            <span className="text-disabled uppercase tracking-wider not-italic font-sans text-[10px] font-semibold">Reserved</span>
            <span title="Reserved CPU">{res!.cpu} <span className="text-disabled">CPU</span></span>
            <span className="text-disabled/40">·</span>
            <span title="Reserved memory">{formatBytes(res!.memory_mb)} <span className="text-disabled">RAM</span></span>
            <span className="text-disabled/40">·</span>
            <span title="Reserved disk">{formatBytes(res!.disk_mb)} <span className="text-disabled">Disk</span></span>
          </div>
        )}
      </div>
      {/* Two groups side-by-side on wider screens to keep the panel short. */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <TimerGroup title={`Stage timeouts${sortedJobs.length ? ` · ${sortedJobs.length}` : ''}`}>
          {sortedJobs.length > 0 ? (
            sortedJobs.map(j => <StageTimerRow key={j.id} job={j} />)
          ) : (
            <p className="text-xs text-disabled italic">No stages submitted yet</p>
          )}
        </TimerGroup>

        <TimerGroup title="Build timeouts">
          <TimerRow label="Stall timeout" timer={stallTimer} />
          <TimerRow label="Idle timeout" timer={idleTimer} />
        </TimerGroup>
      </div>
    </div>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

type DialogKind = 'cancel' | 'retry-build' | 'retry-job' | null;

// Silence unused-import linter for formatDuration (kept for potential callers).
void formatDuration;

export default function BuildDetailPage() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { canCancelJobs } = usePermission();
  const [dialog, setDialog] = useState<DialogKind>(null);
  const [retryJobTarget, setRetryJobTarget] = useState<Job | null>(null);
  const { data, isLoading, isError } = useQuery({
    queryKey: ['build', id],
    queryFn: () => getBuild(id!),
    enabled: !!id,
    refetchInterval: (query) => {
      const state = query.state.data?.job_group?.state;
      if (state === 'success' || state === 'failed' || state === 'cancelled') return false;
      return 3000;
    },
  });

  const cancelMutation = useMutation({
    mutationFn: () => cancelBuild(id!, 'Cancelled from dashboard'),
    onSuccess: () => {
      toast.success('Build cancelled');
      qc.invalidateQueries({ queryKey: ['build', id] });
      setDialog(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to cancel build'),
  });

  const retryBuildMutation = useMutation({
    mutationFn: () => retryBuild(id!),
    onSuccess: () => {
      toast.success('Build retried');
      qc.invalidateQueries({ queryKey: ['build', id] });
      setDialog(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to retry build'),
  });

  const retryJobMutation = useMutation({
    mutationFn: (jobId: string) => retryJob(jobId),
    onSuccess: () => {
      toast.success('Stage retried');
      qc.invalidateQueries({ queryKey: ['build', id] });
      setDialog(null);
      setRetryJobTarget(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to retry stage'),
  });

  if (isLoading) return <PageSkeleton rows={4} />;
  if (isError) return (
    <div role="alert" className="bg-danger-soft border border-danger/30 rounded-lg p-4 text-danger">
      Failed to load build. Please try again.
    </div>
  );
  if (!data) return <div className="text-muted">Build not found</div>;

  const { job_group: group, jobs } = data;
  const isTerminal = ['success', 'failed', 'cancelled'].includes(group.state);

  function openRetryJob(job: Job) {
    setRetryJobTarget(job);
    setDialog('retry-job');
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-4 flex-wrap">
        <button
          onClick={() => nav('/builds')}
          aria-label="Back to builds list"
          className="text-muted hover:text-primary transition-colors text-sm flex items-center gap-1 focus:outline-none focus:ring-2 focus:ring-accent rounded"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
          Builds
        </button>
        <h2 className="text-2xl font-bold text-primary font-mono">{group.job_group_id.slice(0, 8)}</h2>
        <StatusBadge status={group.state} size="md" />

        {/* Archived / files-purged badges */}
        {group.archived && (
          <StatusBadge
            status="archived"
            size="md"
            title={group.archived_at ? `DB row in archive table since ${fmtDate(group.archived_at)}` : 'Archived'}
          />
        )}
        {group.files_purged_at && (
          <StatusBadge
            status="files-purged"
            size="md"
            title={`Logs and workspace removed on ${fmtDate(group.files_purged_at)}`}
          />
        )}

        {group.status_reason && (
          <p className="text-xs text-muted mt-1">{group.status_reason}</p>
        )}
        <div className="ml-auto flex items-center gap-2">
          {canCancelJobs && group.state === 'failed' && (
            <button
              onClick={() => setDialog('retry-build')}
              className="px-4 py-2 text-sm bg-warning-soft text-warning border border-warning/30 rounded-lg hover:opacity-80 transition-colors focus:outline-none focus:ring-2 focus:ring-warning"
            >
              Retry Build
            </button>
          )}
          {canCancelJobs && !isTerminal && (
            <button
              onClick={() => setDialog('cancel')}
              className="px-4 py-2 text-sm bg-danger/20 text-danger border border-danger/30 rounded-lg hover:bg-danger/30 transition-colors focus:outline-none focus:ring-2 focus:ring-danger"
            >
              Cancel Build
            </button>
          )}
        </div>
      </div>

      {/* Meta grid */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="bg-surface border border-border rounded-lg p-3">
          <p className="text-xs text-disabled">Branch</p>
          <p className="text-sm text-secondary">{group.branch || '-'}</p>
        </div>
        <div className="bg-surface border border-border rounded-lg p-3">
          <p className="text-xs text-disabled">Commit</p>
          <p className="text-sm text-secondary font-mono">{group.commit_sha?.slice(0, 7) || '-'}</p>
        </div>
        <div className="bg-surface border border-border rounded-lg p-3">
          <p className="text-xs text-disabled">Worker</p>
          <p className="text-sm text-secondary truncate">{group.reserved_worker_id || jobs?.[0]?.worker_id || '-'}</p>
        </div>
        <div className="bg-surface border border-border rounded-lg p-3">
          <p className="text-xs text-disabled">Created</p>
          <p className="text-sm text-secondary">
            <TimeAgo date={group.created_at} />
          </p>
        </div>
      </div>

      {/* Timers + reserved resources (combined to save vertical space) */}
      <TimersPanel group={group} jobs={jobs} />

      {/* Reserved stages manifest — shows every stage that was granted at
          reservation time and its current state (Pending until submitted). */}
      {group.reserved_stages && group.reserved_stages.length > 0 && (
        <div className="bg-surface border border-border rounded-xl">
          <div className="px-4 py-3 border-b border-border flex items-center justify-between">
            <h3 className="text-sm font-semibold text-secondary">
              Reserved stages ({jobs.length} / {group.reserved_stages.length} submitted)
            </h3>
            {jobs.length < group.reserved_stages.length && !isTerminal && (
              <span className="text-xs text-muted">Waiting for {group.reserved_stages.length - jobs.length} more</span>
            )}
          </div>
          <div className="px-4 py-3 flex flex-wrap gap-2">
            {group.reserved_stages.map((stageName) => {
              const job = jobs.find((j) => j.stage_name === stageName);
              return (
                <div
                  key={stageName}
                  className="inline-flex items-center gap-2 bg-surface-2/50 border border-border rounded-lg pl-3 pr-2 py-1.5"
                >
                  <span className="text-sm text-secondary font-mono">{stageName}</span>
                  <StatusBadge status={job ? job.state : 'pending'} />
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Pipeline explorer: left accordion tree (stages -> pre/cmd/post,
          global post-script) + right output pane for the selected step. */}
      <PipelineExplorer
        jobs={jobs}
        group={group}
        filesPurgedAt={group.files_purged_at}
        onRetryJob={canCancelJobs ? openRetryJob : undefined}
      />

      {/* Archived child records (collapsed by default) */}
      {group.archived && group.children && (
        <ArchivedChildrenPanel children={group.children} />
      )}

      <ConfirmDialog
        open={dialog === 'cancel'}
        title="Cancel Build"
        message="Are you sure you want to cancel this build? Running stages will be terminated."
        confirmLabel="Cancel Build"
        variant="danger"
        onConfirm={() => cancelMutation.mutate()}
        onCancel={() => setDialog(null)}
      />
      <ConfirmDialog
        open={dialog === 'retry-build'}
        title="Retry Build"
        message="Re-run this build from scratch? A new job group will be created."
        confirmLabel="Retry Build"
        onConfirm={() => retryBuildMutation.mutate()}
        onCancel={() => setDialog(null)}
      />
      <ConfirmDialog
        open={dialog === 'retry-job'}
        title="Retry Stage"
        message={`Retry the "${retryJobTarget?.stage_name}" stage?`}
        confirmLabel="Retry Stage"
        onConfirm={() => retryJobTarget && retryJobMutation.mutate(retryJobTarget.id)}
        onCancel={() => { setDialog(null); setRetryJobTarget(null); }}
      />
    </div>
  );
}
