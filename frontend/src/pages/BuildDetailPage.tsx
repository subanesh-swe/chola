import { useState } from 'react';
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
import type { Job, MutationError } from '../types';

interface JobLogPanelProps {
  job: Job;
  onRetry?: () => void;
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
          <span>{formatDuration(job.started_at, job.completed_at)}</span>
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
