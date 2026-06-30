import { useMemo, useState } from 'react';
import { clsx } from 'clsx';
import { useQuery } from '@tanstack/react-query';
import apiClient from '../../api/client';
import type { Job, JobGroup } from '../../types';
import { StatusBadge } from '../ui/StatusBadge';
import { LogViewer } from '../log/LogViewer';
import { useLiveLog } from '../../hooks/useLiveLog';
import { PipelineGraph } from './PipelineGraph';
import {
  buildPipelineModel,
  parseLogSections,
  type PipelineLeaf,
  type StepRef,
} from '../../lib/pipeline';

interface Props {
  jobs: Job[];
  group: JobGroup;
  filesPurgedAt?: string | null;
  /** When set, a "Retry stage" button shows for the selected failed stage. */
  onRetryJob?: (job: Job) => void;
}

// ── Right pane: output for the selected job step ────────────────────────────

function LeafOutput({
  job,
  kind,
  filesPurgedAt,
}: {
  job: Job;
  kind: PipelineLeaf['kind'];
  filesPurgedAt?: string | null;
}) {
  const isRunning = job.state === 'running' || job.state === 'assigned';
  const isPurged = !!filesPurgedAt;

  const { chunks } = useLiveLog(job.id, isRunning && !isPurged);

  const { data: logData } = useQuery({
    queryKey: ['job-logs', job.id],
    queryFn: () => apiClient.get(`/jobs/${job.id}/logs`).then((r) => r.data),
    enabled: !isRunning && !!job.id && !isPurged,
  });

  const completedFull: string = logData?.data || '';
  const section = useMemo(
    () => (isRunning ? '' : parseLogSections(completedFull)[kind]),
    [isRunning, completedFull, kind],
  );

  return (
    <LogViewer
      key={`${job.id}:${kind}:${isRunning ? 'live' : 'done'}`}
      content={isRunning ? undefined : (section || `(no ${kind} output)`)}
      liveChunks={isRunning ? chunks : undefined}
      filesPurgedAt={filesPurgedAt}
      className="h-full"
    />
  );
}

// ── Explorer ────────────────────────────────────────────────────────────────

export function PipelineExplorer({ jobs, group, filesPurgedAt, onRetryJob }: Props) {
  const model = useMemo(() => buildPipelineModel(jobs, group), [jobs, group]);

  const initial = useMemo<StepRef | null>(() => {
    if (!model.stages.length) return null;
    const running = model.stages.find((n) => n.state === 'running');
    const target = running ?? model.stages[model.stages.length - 1];
    const cmd = target.leaves.find((l) => l.kind === 'cmd') ?? target.leaves[0];
    return cmd
      ? {
          type: 'job',
          key: cmd.key,
          jobId: cmd.jobId,
          kind: cmd.kind,
          label: cmd.label,
          state: cmd.state,
          exitCode: cmd.exitCode,
        }
      : null;
  }, [model]);

  const [selection, setSelection] = useState<StepRef | null>(initial);
  const active = selection ?? initial;

  const selectedJob =
    active?.type === 'job' ? jobs.find((j) => j.id === active.jobId) ?? null : null;

  if (!model.stages.length && !model.globalPre.present && !model.globalPost.present) {
    return (
      <div className="bg-surface border border-border rounded-xl px-4 py-8 text-center text-disabled">
        No stages submitted yet
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[minmax(260px,340px)_1fr] gap-3">
      {/* Left: pipeline graph */}
      <div className="bg-surface border border-border rounded-xl flex flex-col min-h-[28rem] max-h-[36rem]">
        <div className="px-4 py-3 border-b border-border shrink-0">
          <h3 className="text-sm font-semibold text-secondary">Pipeline</h3>
        </div>
        <div className="p-3 overflow-y-auto">
          <PipelineGraph model={model} selectedKey={active?.key} onSelect={setSelection} />
        </div>
      </div>

      {/* Right: output */}
      <div className="bg-surface border border-border rounded-xl flex flex-col min-h-[28rem] max-h-[36rem]">
        <div className="px-4 py-3 border-b border-border flex items-center gap-3 shrink-0 flex-wrap">
          <h3 className="text-sm font-semibold text-secondary truncate">
            {active ? active.label : 'Output'}
          </h3>
          {selectedJob?.state === 'failed' && onRetryJob && (
            <button
              onClick={() => onRetryJob(selectedJob)}
              className="px-3 py-1 text-xs bg-warning-soft text-warning border border-warning/30 rounded-lg hover:opacity-80 transition-colors focus:outline-none focus:ring-2 focus:ring-warning"
            >
              Retry stage
            </button>
          )}
          {active && (
            <span className="ml-auto flex items-center gap-2 shrink-0">
              {active.type === 'job' && active.exitCode !== null && (
                <span
                  className={clsx(
                    'font-mono text-xs',
                    active.exitCode === 0 ? 'text-disabled' : 'text-danger',
                  )}
                >
                  exit {active.exitCode}
                </span>
              )}
              <StatusBadge status={active.state} />
            </span>
          )}
        </div>
        <div className="flex-1 p-2 overflow-hidden">
          {active?.type === 'job' && selectedJob ? (
            <LeafOutput job={selectedJob} kind={active.kind} filesPurgedAt={filesPurgedAt} />
          ) : active?.type === 'info' ? (
            <div className="h-full flex items-center justify-center text-center text-disabled text-sm px-6">
              {active.message}
            </div>
          ) : (
            <div className="h-full flex items-center justify-center text-disabled text-sm">
              Select a step on the left
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
