import { useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getBuild, cancelBuild } from '../api/builds';
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
import type { Job } from '../types';

function JobLogPanel({ job }: { job: Job }) {
  const isRunning = job.state === 'running' || job.state === 'assigned';
  const { chunks } = useLiveLog(job.id, isRunning);

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
        </div>
      </div>
      <StageMetadata job={job} />
      <div className="px-4 pb-4">
        <LogViewer
          content={isRunning ? undefined : `Stage: ${job.stage_name}\nState: ${job.state}\n---\n`}
          liveChunks={isRunning ? chunks : undefined}
          className="h-80"
        />
      </div>
    </div>
  );
}

export default function BuildDetailPage() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { canCancelJobs } = usePermission();
  const [showCancel, setShowCancel] = useState(false);
  const [selectedJob, setSelectedJob] = useState<Job | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ['build', id],
    queryFn: () => getBuild(id!),
    enabled: !!id,
    refetchInterval: 3000,
  });

  const cancelMutation = useMutation({
    mutationFn: () => cancelBuild(id!, 'Cancelled from dashboard'),
    onSuccess: () => {
      toast.success('Build cancelled');
      qc.invalidateQueries({ queryKey: ['build', id] });
      setShowCancel(false);
    },
    onError: () => toast.error('Failed to cancel build'),
  });

  if (isLoading) return <div className="text-slate-400">Loading...</div>;
  if (!data) return <div className="text-slate-400">Build not found</div>;

  const { job_group: group, jobs } = data;
  const isTerminal = ['success', 'failed', 'cancelled'].includes(group.state);

  // Auto-select first running or failed job if nothing selected
  const activeSelectedJob =
    selectedJob ?? jobs.find((j) => j.state === 'running') ?? jobs.find((j) => j.state === 'failed') ?? null;

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
        {canCancelJobs && !isTerminal && (
          <button
            onClick={() => setShowCancel(true)}
            className="ml-auto px-4 py-2 text-sm bg-red-600/20 text-red-400 border border-red-500/30 rounded-lg hover:bg-red-600/30 transition-colors focus:outline-none focus:ring-2 focus:ring-red-500"
          >
            Cancel Build
          </button>
        )}
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
          <p className="text-sm text-slate-200 truncate">{group.reserved_worker_id || '-'}</p>
        </div>
        <div className="bg-slate-900 border border-slate-700 rounded-lg p-3">
          <p className="text-xs text-slate-500">Created</p>
          <p className="text-sm text-slate-200">
            <TimeAgo date={group.created_at} />
          </p>
        </div>
      </div>

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
      {activeSelectedJob && <JobLogPanel key={activeSelectedJob.id} job={activeSelectedJob} />}

      <ConfirmDialog
        open={showCancel}
        title="Cancel Build"
        message="Are you sure you want to cancel this build? Running stages will be terminated."
        confirmLabel="Cancel Build"
        variant="danger"
        onConfirm={() => cancelMutation.mutate()}
        onCancel={() => setShowCancel(false)}
      />
    </div>
  );
}
