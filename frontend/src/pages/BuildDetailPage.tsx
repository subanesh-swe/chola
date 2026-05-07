import { useState, useEffect } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getBuild, cancelBuild, retryBuild, retryJob } from '../api/builds';
import apiClient from '../api/client';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { LogViewer } from '../components/log/LogViewer';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { StageTimeline } from '../components/ui/StageTimeline';
import { StageMetadata } from '../components/ui/StageMetadata';
import { useLiveLog } from '../hooks/useLiveLog';
import { usePermission } from '../hooks/usePermission';
import { formatDuration } from '../utils/duration';
import { formatSecs, formatBytes } from '../utils/format';
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

interface JobLogPanelProps {
  job: Job;
  filesPurgedAt: string | null | undefined;
  onRetry?: () => void;
}

function StageTimer({ job }: { job: Job }) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    if (job.state !== 'running' || !job.started_at) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [job.state, job.started_at]);

  if (!job.started_at) return null;

  const startMs = new Date(job.started_at).getTime();
  const endMs = job.completed_at ? new Date(job.completed_at).getTime() : now;
  const elapsedSecs = Math.max(0, Math.floor((endMs - startMs) / 1000));
  const maxSecs = job.max_duration_secs || 0;

  const h = Math.floor(elapsedSecs / 3600);
  const m = Math.floor((elapsedSecs % 3600) / 60);
  const s = elapsedSecs % 60;
  const elapsed = h > 0 ? `${h}h ${m}m ${s}s` : m > 0 ? `${m}m ${s}s` : `${s}s`;

  const maxH = Math.floor(maxSecs / 3600);
  const maxM = Math.floor((maxSecs % 3600) / 60);
  const maxLabel = maxSecs > 0
    ? (maxH > 0 ? `${maxH}h ${maxM}m` : `${maxM}m`)
    : null;

  const pct = maxSecs > 0 ? elapsedSecs / maxSecs : 0;
  const color = pct > 0.9 ? 'text-red-400' : pct > 0.7 ? 'text-yellow-400' : 'text-muted';

  return (
    <span className={`text-xs font-mono ${color}`}>
      {elapsed}{maxLabel && ` / ${maxLabel}`}
    </span>
  );
}

function JobLogPanel({ job, filesPurgedAt, onRetry }: JobLogPanelProps) {
  const isRunning = job.state === 'running' || job.state === 'assigned';
  const { chunks } = useLiveLog(job.id, isRunning && !filesPurgedAt);

  // For completed jobs, fetch logs via GET — but skip if files are gone.
  const { data: logData } = useQuery({
    queryKey: ['job-logs', job.id],
    queryFn: () => apiClient.get(`/jobs/${job.id}/logs`).then((r) => r.data),
    enabled: !isRunning && !!job.id && !filesPurgedAt,
  });

  const completedLogs = logData?.data || '';

  return (
    <div className="bg-surface border border-border rounded-xl">
      <div className="px-4 py-3 border-b border-border flex items-center justify-between">
        <div className="flex items-center gap-3">
          <StatusBadge status={job.state} />
          <h3 className="text-sm font-semibold text-secondary">{job.stage_name}</h3>
        </div>
        <div className="flex items-center gap-4 text-xs text-muted">
          {job.exit_code !== null && <span>exit: {job.exit_code}</span>}
          {job.status_reason && <span className="text-xs text-disabled">{job.status_reason}</span>}
          <StageTimer job={job} />
          {job.state === 'failed' && onRetry && (
            <button
              onClick={onRetry}
              className="px-3 py-1 text-xs bg-yellow-600/20 text-yellow-400 border border-yellow-500/30 rounded-lg hover:bg-yellow-600/30 transition-colors focus:outline-none focus:ring-2 focus:ring-yellow-500"
            >
              Retry Stage
            </button>
          )}
        </div>
      </div>
      <StageMetadata job={job} />
      <div className="px-4 pb-4">
        <LogViewer
          content={isRunning ? undefined : completedLogs || `Stage: ${job.stage_name}\nState: ${job.state}\n`}
          liveChunks={isRunning ? chunks : undefined}
          filesPurgedAt={filesPurgedAt}
          className="h-80"
        />
      </div>
    </div>
  );
}

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
  const color = status === 'active' ? 'text-emerald-400' : status === 'paused' ? 'text-amber-400' : 'text-slate-600';
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
        <span className="text-slate-200 font-mono">{timeDisplay}</span>
        {/* item 19: truncate + max-w-xs on timer reason */}
        <span className={`${color} max-w-xs text-right truncate`} title={reason}>{reason}</span>
      </div>
    </div>
  );
}

function TimersPanel({ group, jobs }: { group: JobGroup & { timers?: { idle?: TimerInfo; stall?: TimerInfo; stage?: TimerInfo } }; jobs: Job[] }) {
  const isTerminal = ['success', 'failed', 'cancelled', 'expired'].includes(group.state);
  if (isTerminal) return null;

  const runningJob = jobs.find(j => j.state === 'running') ?? null;

  if (group.timers) {
    return (
      <div className="bg-surface border border-border rounded-xl p-4">
        <h3 className="text-xs font-semibold text-disabled uppercase tracking-wider mb-3">Timers</h3>
        <div className="space-y-2">
          <TimerRow label="Stage timeout" timer={group.timers.stage} job={runningJob} />
          <TimerRow label="Stall timeout" timer={group.timers.stall} />
          <TimerRow label="Idle timeout" timer={group.timers.idle} />
        </div>
      </div>
    );
  }

  const hasRunning = jobs.some(j => j.state === 'running' || j.state === 'assigned');
  const idleMax = group.idle_timeout_secs ?? 300;
  const stallMax = group.stall_timeout_secs ?? 1800;

  const stageTimer: TimerInfo = runningJob && runningJob.max_duration_secs > 0 && runningJob.started_at
    ? { status: 'active', remaining_secs: runningJob.max_duration_secs - Math.floor((Date.now() - new Date(runningJob.started_at).getTime()) / 1000), max_secs: runningJob.max_duration_secs, reason: `Active (${runningJob.stage_name})` }
    : { status: 'na', remaining_secs: null, max_secs: runningJob?.max_duration_secs ?? 0 };

  const stallTimer: TimerInfo = group.state === 'running'
    ? hasRunning
      ? { status: 'paused', remaining_secs: null, max_secs: stallMax, reason: 'Paused (stage running)' }
      : { status: 'active', remaining_secs: group.time_until_timeout_secs ?? stallMax, max_secs: stallMax, reason: 'Waiting for next stage' }
    : { status: 'na', remaining_secs: null, max_secs: stallMax };

  const idleTimer: TimerInfo = group.state === 'reserved'
    ? { status: 'active', remaining_secs: group.time_until_timeout_secs ?? idleMax, max_secs: idleMax, reason: 'Waiting for first stage' }
    : { status: 'deactivated', remaining_secs: null, max_secs: idleMax };

  return (
    <div className="bg-surface border border-border rounded-xl p-4">
      <h3 className="text-xs font-semibold text-disabled uppercase tracking-wider mb-3">Timers</h3>
      <div className="space-y-2">
        <TimerRow label="Stage timeout" timer={stageTimer} />
        <TimerRow label="Stall timeout" timer={stallTimer} />
        <TimerRow label="Idle timeout" timer={idleTimer} />
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
  const [selectedJob, setSelectedJob] = useState<Job | null>(null);

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

  // item 15: use skeleton instead of plain text
  if (isLoading) return <PageSkeleton rows={4} />;
  if (isError) return (
    <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
      Failed to load build. Please try again.
    </div>
  );
  if (!data) return <div className="text-muted">Build not found</div>;

  const { job_group: group, jobs } = data;
  const isTerminal = ['success', 'failed', 'cancelled'].includes(group.state);

  const activeSelectedJob =
    selectedJob ?? jobs.find((j) => j.state === 'running') ?? jobs.find((j) => j.state === 'failed') ?? null;

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
              className="px-4 py-2 text-sm bg-yellow-600/20 text-yellow-400 border border-yellow-500/30 rounded-lg hover:bg-yellow-600/30 transition-colors focus:outline-none focus:ring-2 focus:ring-yellow-500"
            >
              Retry Build
            </button>
          )}
          {canCancelJobs && !isTerminal && (
            <button
              onClick={() => setDialog('cancel')}
              className="px-4 py-2 text-sm bg-red-600/20 text-red-400 border border-red-500/30 rounded-lg hover:bg-red-600/30 transition-colors focus:outline-none focus:ring-2 focus:ring-red-500"
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

      {/* Reserved resources */}
      {group.allocated_resources && (group.allocated_resources.cpu > 0 || group.allocated_resources.memory_mb > 0 || group.allocated_resources.disk_mb > 0) && (
        <div className="bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-3">
          <p className="text-xs text-slate-500 mb-1.5 uppercase font-semibold">Resources Reserved</p>
          {/* item 54: unit labels */}
          <div className="flex gap-6 text-sm">
            <span className="text-slate-300">{group.allocated_resources.cpu} <span className="text-slate-500">CPU</span></span>
            <span className="text-slate-300">{formatBytes(group.allocated_resources.memory_mb)} <span className="text-slate-500">RAM</span></span>
            <span className="text-slate-300">{formatBytes(group.allocated_resources.disk_mb)} <span className="text-slate-500">Disk</span></span>
          </div>
        </div>
      )}

      {/* Timers panel */}
      <TimersPanel group={group} jobs={jobs} />

      {/* Stage pipeline timeline */}
      <div className="bg-surface border border-border rounded-xl">
        <div className="px-4 py-3 border-b border-border">
          <h3 className="text-sm font-semibold text-secondary">
            Pipeline ({jobs.length} stage{jobs.length !== 1 ? 's' : ''})
          </h3>
        </div>
        {jobs.length > 0 ? (
          <div className="p-2">
            <StageTimeline
              jobs={jobs}
              onSelectJob={(job) => setSelectedJob((prev) => (prev?.id === job.id ? null : job))}
              selectedJobId={activeSelectedJob?.id}
            />
          </div>
        ) : (
          <div className="px-4 py-8 text-center text-disabled">No stages submitted yet</div>
        )}
      </div>

      {/* Log panel for selected job */}
      {activeSelectedJob && (
        <JobLogPanel
          key={activeSelectedJob.id}
          job={activeSelectedJob}
          filesPurgedAt={group.files_purged_at}
          onRetry={canCancelJobs ? () => openRetryJob(activeSelectedJob) : undefined}
        />
      )}

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
