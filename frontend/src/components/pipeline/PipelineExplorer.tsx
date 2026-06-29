import { useMemo, useState } from 'react';
import { clsx } from 'clsx';
import { useQuery } from '@tanstack/react-query';
import apiClient from '../../api/client';
import type { Job } from '../../types';
import { StatusBadge } from '../ui/StatusBadge';
import { LogViewer } from '../log/LogViewer';
import { useLiveLog } from '../../hooks/useLiveLog';
import { PipelineGraph } from './PipelineGraph';
import {
  buildPipeline,
  parseLogSections,
  type PipelineLeaf,
  type PipelineNode,
} from '../../lib/pipeline';

interface Props {
  jobs: Job[];
  filesPurgedAt?: string | null;
  /** When set, a "Retry stage" button shows for the selected failed stage. */
  onRetryJob?: (job: Job) => void;
}

interface Selection {
  jobId: string;
  kind: PipelineLeaf['kind'];
}

// ── Right pane: output for the selected leaf ────────────────────────────────

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

  // While running we stream the full live log (slicing a growing stream is
  // noisy); once complete we show just the selected section.
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

// ── Right header: pre/cmd/post tabs for the selected node ────────────────────

function StepTabs({
  leaves,
  activeKind,
  onSelect,
}: {
  leaves: PipelineLeaf[];
  activeKind: PipelineLeaf['kind'];
  onSelect: (kind: PipelineLeaf['kind']) => void;
}) {
  return (
    <div className="flex items-center gap-1">
      {leaves.map((leaf) => (
        <button
          key={leaf.key}
          onClick={() => onSelect(leaf.kind)}
          className={clsx(
            'px-2.5 py-1 text-xs rounded-md transition-colors focus:outline-none focus:ring-2 focus:ring-accent',
            leaf.kind === activeKind
              ? 'bg-accent-soft text-accent-text'
              : 'text-muted hover:bg-surface-hover/50',
          )}
        >
          {leaf.label}
        </button>
      ))}
    </div>
  );
}

// ── Explorer ────────────────────────────────────────────────────────────────

export function PipelineExplorer({ jobs, filesPurgedAt, onRetryJob }: Props) {
  const nodes = useMemo(() => buildPipeline(jobs), [jobs]);

  const initial = useMemo<Selection | null>(() => {
    if (!nodes.length) return null;
    const running = nodes.find((n) => n.state === 'running');
    const target = running ?? nodes[nodes.length - 1];
    const cmd = target.leaves.find((l) => l.kind === 'cmd') ?? target.leaves[0];
    return cmd ? { jobId: target.job.id, kind: cmd.kind } : null;
  }, [nodes]);

  const [selection, setSelection] = useState<Selection | null>(initial);
  const active = selection ?? initial;

  const activeNode: PipelineNode | null = active
    ? nodes.find((n) => n.job.id === active.jobId) ?? null
    : null;
  const selectedJob = activeNode?.job ?? null;
  const activeLeaf = activeNode?.leaves.find((l) => l.kind === active?.kind) ?? null;

  if (!nodes.length) {
    return (
      <div className="bg-surface border border-border rounded-xl px-4 py-8 text-center text-disabled">
        No stages submitted yet
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[minmax(240px,320px)_1fr] gap-3">
      {/* Left: pipeline graph (Blue-Ocean style, parallel split/join) */}
      <div className="bg-surface border border-border rounded-xl flex flex-col min-h-[28rem] max-h-[36rem]">
        <div className="px-4 py-3 border-b border-border shrink-0">
          <h3 className="text-sm font-semibold text-secondary">Pipeline</h3>
        </div>
        <div className="p-4 overflow-y-auto">
          <PipelineGraph
            nodes={nodes}
            selectedJobId={selectedJob?.id}
            onSelectNode={(node) => {
              const cmd = node.leaves.find((l) => l.kind === 'cmd') ?? node.leaves[0];
              if (cmd) setSelection({ jobId: node.job.id, kind: cmd.kind });
            }}
          />
        </div>
      </div>

      {/* Right: output */}
      <div className="bg-surface border border-border rounded-xl flex flex-col min-h-[28rem] max-h-[36rem]">
        <div className="px-4 py-3 border-b border-border flex items-center gap-3 shrink-0 flex-wrap">
          <h3 className="text-sm font-semibold text-secondary truncate">
            {selectedJob ? selectedJob.stage_name : 'Output'}
          </h3>
          {activeNode && active && (
            <StepTabs
              leaves={activeNode.leaves}
              activeKind={active.kind}
              onSelect={(kind) => setSelection({ jobId: activeNode.job.id, kind })}
            />
          )}
          {selectedJob?.state === 'failed' && onRetryJob && (
            <button
              onClick={() => onRetryJob(selectedJob)}
              className="px-3 py-1 text-xs bg-warning-soft text-warning border border-warning/30 rounded-lg hover:opacity-80 transition-colors focus:outline-none focus:ring-2 focus:ring-warning"
            >
              Retry stage
            </button>
          )}
          {activeLeaf && (
            <span className="ml-auto flex items-center gap-2 shrink-0">
              {activeLeaf.exitCode !== null && (
                <span
                  className={clsx(
                    'font-mono text-xs',
                    activeLeaf.exitCode === 0 ? 'text-disabled' : 'text-danger',
                  )}
                >
                  exit {activeLeaf.exitCode}
                </span>
              )}
              <StatusBadge status={activeLeaf.state} />
            </span>
          )}
        </div>
        <div className="flex-1 p-2 overflow-hidden">
          {selectedJob && active ? (
            <LeafOutput job={selectedJob} kind={active.kind} filesPurgedAt={filesPurgedAt} />
          ) : (
            <div className="h-full flex items-center justify-center text-disabled text-sm">
              Select a stage on the left
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
