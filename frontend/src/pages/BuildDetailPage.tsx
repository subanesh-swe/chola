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
import { toast } from 'sonner';
import type { Job, JobGroup, MutationError } from '../types';

interface JobLogPanelProps {
  job: Job;
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
  const color = pct > 0.9 ? 'text-red-400' : pct > 0.7 ? 'text-yellow-400' : 'text-slate-400';

  return (
    <span className={`text-xs font-mono ${color}`}>
      {elapsed}{maxLabel && ` / ${maxLabel}`}
    </span>
  );
}

function JobLogPanel({ job, onRetry }: JobLogPanelProps) {
  const isRunning = job.state === 'running' || job.state === 'assigned';
  const { chunks } = useLiveLog(job.id, isRunning);

  // For completed jobs, fetch logs via GET
  const { data: logData } = useQuery({
    queryKey: ['job-logs', job.id],
    queryFn: () => apiClient.get(`/jobs/${job.id}/logs`).then((r) => r.data),
    enabled: !isRunning && !!job.id,
  });

  const completedLogs = logData?.data || '';

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl">
      <div className="px-4 py-3 border-b border-slate-700 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <StatusBadge status={job.state} />
          <h3 className="text-sm font-semibold text-slate-300">{job.stage_name}</h3>
        </div>
        <div className="flex items-center gap-4 text-xs text-slate-400">
          {job.exit_code !== null && <span>exit: {job.exit_code}</span>}
          {job.status_reason && <span className="text-xs text-slate-500">{job.status_reason}</span>}
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
          className="h-80"
        />
      </div>
    </div>
  );
}

function fmtSecs(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
}

function TimersPanel({ group, jobs }: { group: JobGroup; jobs: Job[] }) {
  const isTerminal = ['success', 'failed', 'cancelled', 'expired'].includes(group.state);
  if (isTerminal) return null;

  const hasRunningStage = jobs.some(j => j.state === 'running' || j.state === 'assigned');
  const runningJob = jobs.find(j => j.state === 'running');
  const idleTimeout = group.idle_timeout_secs ?? 300;
  const stallTimeout = group.stall_timeout_secs ?? 1800;
  const timeUntil = group.time_until_timeout_secs ?? -1;

  // Stage timeout
  let stageStatus: 'active' | 'paused' | 'na' = 'na';
  let stageRemaining = '';
  let stageMax = '';
  if (runningJob && runningJob.max_duration_secs > 0 && runningJob.started_at) {
    stageStatus = 'active';
    const elapsed = Math.floor((Date.now() - new Date(runningJob.started_at).getTime()) / 1000);
    const remaining = Math.max(0, runningJob.max_duration_secs - elapsed);
    stageRemaining = fmtSecs(remaining);
    stageMax = fmtSecs(runningJob.max_duration_secs);
  } else if (runningJob) {
    stageStatus = 'active';
    stageRemaining = 'no limit';
  }

  // Idle timeout — deactivated once any stage starts
  let idleStatus: 'active' | 'paused' | 'na' | 'deactivated' = 'na';
  let idleRemaining = '';
  if (group.state === 'reserved') {
    idleStatus = 'active';
    idleRemaining = timeUntil >= 0 ? fmtSecs(timeUntil) : fmtSecs(idleTimeout);
  } else if (group.state === 'running') {
    idleStatus = 'deactivated';
  }

  // Stall timeout
  let stallStatus: 'active' | 'paused' | 'na' = 'na';
  let stallRemaining = '';
  if (group.state === 'running') {
    if (hasRunningStage) {
      stallStatus = 'paused';
    } else {
      stallStatus = 'active';
      stallRemaining = timeUntil >= 0 ? fmtSecs(timeUntil) : fmtSecs(stallTimeout);
    }
  }

  const statusIcon = (s: 'active' | 'paused' | 'na' | 'deactivated') =>
    s === 'active' ? '⏱' : s === 'paused' ? '⏸' : s === 'deactivated' ? '✓' : '○';
  const statusColor = (s: 'active' | 'paused' | 'na' | 'deactivated') =>
    s === 'active' ? 'text-emerald-400' : s === 'paused' ? 'text-slate-500' : s === 'deactivated' ? 'text-slate-600' : 'text-slate-600';
  const statusLabel = (s: 'active' | 'paused' | 'na' | 'deactivated', reason: string) =>
    s === 'active' ? reason : s === 'paused' ? 'Paused (stage running)' : s === 'deactivated' ? 'Deactivated' : 'N/A';

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl p-4">
      <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Timers</h3>
      <div className="space-y-2">
        {/* Stage timeout */}
        <div className="flex items-center justify-between text-xs">
          <div className="flex items-center gap-2">
            <span>{statusIcon(stageStatus)}</span>
            <span className="text-slate-300">Stage timeout</span>
          </div>
          <div className="flex items-center gap-3">
            {stageStatus === 'active' ? (
              <span className="text-slate-200 font-mono">{stageRemaining}{stageMax ? ` / ${stageMax}` : ''}</span>
            ) : (
              <span className="text-slate-600">—</span>
            )}
            <span className={`${statusColor(stageStatus)} w-36 text-right`}>
              {statusLabel(stageStatus, runningJob ? `Active (${runningJob.stage_name})` : 'Active')}
            </span>
          </div>
        </div>

        {/* Stall timeout */}
        <div className="flex items-center justify-between text-xs">
          <div className="flex items-center gap-2">
            <span>{statusIcon(stallStatus)}</span>
            <span className="text-slate-300">Stall timeout</span>
          </div>
          <div className="flex items-center gap-3">
            {stallStatus === 'active' ? (
              <span className="text-slate-200 font-mono">{stallRemaining} / {fmtSecs(stallTimeout)}</span>
            ) : (
              <span className="text-slate-600">—</span>
            )}
            <span className={`${statusColor(stallStatus)} w-36 text-right`}>
              {statusLabel(stallStatus, 'Waiting for next stage')}
            </span>
          </div>
        </div>

        {/* Idle timeout */}
        <div className="flex items-center justify-between text-xs">
          <div className="flex items-center gap-2">
            <span>{statusIcon(idleStatus)}</span>
            <span className="text-slate-300">Idle timeout</span>
          </div>
          <div className="flex items-center gap-3">
            {idleStatus === 'active' ? (
              <span className="text-slate-200 font-mono">{idleRemaining} / {fmtSecs(idleTimeout)}</span>
            ) : (
              <span className="text-slate-600">—</span>
            )}
            <span className={`${statusColor(idleStatus)} w-36 text-right`}>
              {statusLabel(idleStatus, 'Waiting for first stage')}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

type DialogKind = 'cancel' | 'retry-build' | 'retry-job' | null;

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
      // Stop polling for terminal states
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

  if (isLoading) return <div className="text-slate-400">Loading...</div>;
  if (isError) return (
    <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
      Failed to load build. Please try again.
    </div>
  );
  if (!data) return <div className="text-slate-400">Build not found</div>;

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
          className="text-slate-400 hover:text-white transition-colors text-sm flex items-center gap-1 focus:outline-none focus:ring-2 focus:ring-blue-500 rounded"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
          Builds
        </button>
        <h2 className="text-2xl font-bold text-white font-mono">{group.job_group_id.slice(0, 8)}</h2>
        <StatusBadge status={group.state} size="md" />
        {group.status_reason && (
          <p className="text-xs text-slate-400 mt-1">{group.status_reason}</p>
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
        <div className="bg-slate-900 border border-slate-700 rounded-lg p-3">
          <p className="text-xs text-slate-500">Branch</p>
          <p className="text-sm text-slate-200">{group.branch || '-'}</p>
        </div>
        <div className="bg-slate-900 border border-slate-700 rounded-lg p-3">
          <p className="text-xs text-slate-500">Commit</p>
          <p className="text-sm text-slate-200 font-mono">{group.commit_sha?.slice(0, 7) || '-'}</p>
        </div>
        <div className="bg-slate-900 border border-slate-700 rounded-lg p-3">
          <p className="text-xs text-slate-500">Worker</p>
          <p className="text-sm text-slate-200 truncate">{group.reserved_worker_id || jobs?.[0]?.worker_id || '-'}</p>
        </div>
        <div className="bg-slate-900 border border-slate-700 rounded-lg p-3">
          <p className="text-xs text-slate-500">Created</p>
          <p className="text-sm text-slate-200">
            <TimeAgo date={group.created_at} />
          </p>
        </div>
      </div>

      {/* Reserved resources */}
      {group.allocated_resources && (group.allocated_resources.cpu > 0 || group.allocated_resources.memory_mb > 0 || group.allocated_resources.disk_mb > 0) && (
        <div className="bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-3">
          <p className="text-xs text-slate-500 mb-1.5 uppercase font-semibold">Resources Reserved</p>
          <div className="flex gap-6 text-sm">
            <span className="text-slate-300">{group.allocated_resources.cpu} <span className="text-slate-500">CPU</span></span>
            <span className="text-slate-300">{group.allocated_resources.memory_mb >= 1024 ? `${(group.allocated_resources.memory_mb / 1024).toFixed(1)} GB` : `${group.allocated_resources.memory_mb} MB`} <span className="text-slate-500">RAM</span></span>
            <span className="text-slate-300">{group.allocated_resources.disk_mb >= 1024 ? `${(group.allocated_resources.disk_mb / 1024).toFixed(1)} GB` : `${group.allocated_resources.disk_mb} MB`} <span className="text-slate-500">Disk</span></span>
          </div>
        </div>
      )}

      {/* Timers panel */}
      <TimersPanel group={group} jobs={jobs} />

      {/* Stage pipeline timeline */}
      <div className="bg-slate-900 border border-slate-700 rounded-xl">
        <div className="px-4 py-3 border-b border-slate-700">
          <h3 className="text-sm font-semibold text-slate-300">
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
          <div className="px-4 py-8 text-center text-slate-500">No stages submitted yet</div>
        )}
      </div>

      {/* Log panel for selected job */}
      {activeSelectedJob && (
        <JobLogPanel
          key={activeSelectedJob.id}
          job={activeSelectedJob}
          onRetry={canCancelJobs ? () => openRetryJob(activeSelectedJob) : undefined}
        />
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
