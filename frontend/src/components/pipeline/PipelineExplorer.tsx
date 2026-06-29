import { useMemo, useState } from 'react';
import { clsx } from 'clsx';
import { useQuery } from '@tanstack/react-query';
import apiClient from '../../api/client';
import type { Job } from '../../types';
import { StatusBadge } from '../ui/StatusBadge';
import { LogViewer } from '../log/LogViewer';
import { useLiveLog } from '../../hooks/useLiveLog';
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
  leafKey: string;
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
      // Remount when the selected leaf changes so xterm shows fresh content.
      key={`${job.id}:${kind}:${isRunning ? 'live' : 'done'}`}
      content={isRunning ? undefined : (section || `(no ${kind} output)`)}
      liveChunks={isRunning ? chunks : undefined}
      filesPurgedAt={filesPurgedAt}
      className="h-full"
    />
  );
}

// ── Left pane: accordion node + leaves ──────────────────────────────────────

function LeafRow({
  leaf,
  selected,
  onSelect,
}: {
  leaf: PipelineLeaf;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      onClick={onSelect}
      className={clsx(
        'w-full flex items-center justify-between gap-2 pl-9 pr-3 py-1.5 text-left text-xs rounded-md transition-colors',
        'focus:outline-none focus:ring-2 focus:ring-accent',
        selected ? 'bg-accent-soft text-accent-text' : 'hover:bg-surface-hover/50 text-secondary',
      )}
    >
      <span className="font-mono truncate">{leaf.label}</span>
      <span className="flex items-center gap-2 shrink-0">
        {leaf.exitCode !== null && (
          <span
            className={clsx(
              'font-mono text-[10px]',
              leaf.exitCode === 0 ? 'text-disabled' : 'text-danger',
            )}
          >
            exit {leaf.exitCode}
          </span>
        )}
        <StatusBadge status={leaf.state} />
      </span>
    </button>
  );
}

function NodeRow({
  node,
  expanded,
  onToggle,
  selection,
  onSelectLeaf,
}: {
  node: PipelineNode;
  expanded: boolean;
  onToggle: () => void;
  selection: Selection | null;
  onSelectLeaf: (leaf: PipelineLeaf) => void;
}) {
  return (
    <div className="rounded-lg border border-border/60 bg-surface-2/30 overflow-hidden">
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-surface-hover/50 transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
        aria-expanded={expanded}
      >
        <svg
          className={clsx('w-3.5 h-3.5 text-disabled transition-transform shrink-0', expanded && 'rotate-90')}
          fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
        <span className="text-sm font-medium text-secondary truncate flex-1">{node.title}</span>
        <StatusBadge status={node.state} />
      </button>
      {expanded && (
        <div className="px-2 pb-2 pt-0.5 space-y-1">
          {node.leaves.map((leaf) => (
            <LeafRow
              key={leaf.key}
              leaf={leaf}
              selected={selection?.leafKey === leaf.key}
              onSelect={() => onSelectLeaf(leaf)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Explorer ────────────────────────────────────────────────────────────────

export function PipelineExplorer({ jobs, filesPurgedAt, onRetryJob }: Props) {
  const nodes = useMemo(() => buildPipeline(jobs), [jobs]);

  // Default selection: the command leaf of the running stage, else the last
  // stage's command.
  const initial = useMemo<Selection | null>(() => {
    if (!nodes.length) return null;
    const running = nodes.find((n) => n.state === 'running');
    const target = running ?? nodes[nodes.length - 1];
    const cmd = target.leaves.find((l) => l.kind === 'cmd') ?? target.leaves[0];
    return cmd ? { jobId: target.job.id, leafKey: cmd.key, kind: cmd.kind } : null;
  }, [nodes]);

  const [selection, setSelection] = useState<Selection | null>(initial);
  const active = selection ?? initial;

  // Expand the node containing the active selection by default.
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    const s = new Set<string>();
    const owner = nodes.find((n) => n.leaves.some((l) => l.key === initial?.leafKey));
    if (owner) s.add(owner.key);
    return s;
  });

  const selectedJob = active ? jobs.find((j) => j.id === active.jobId) ?? null : null;
  const activeLeaf = active
    ? nodes.flatMap((n) => n.leaves).find((l) => l.key === active.leafKey) ?? null
    : null;

  if (!nodes.length) {
    return (
      <div className="bg-surface border border-border rounded-xl px-4 py-8 text-center text-disabled">
        No stages submitted yet
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[minmax(260px,340px)_1fr] gap-3">
      {/* Left: pipeline tree */}
      <div className="bg-surface border border-border rounded-xl flex flex-col min-h-[28rem] max-h-[36rem]">
        <div className="px-4 py-3 border-b border-border shrink-0">
          <h3 className="text-sm font-semibold text-secondary">Pipeline</h3>
        </div>
        <div className="p-2 space-y-2 overflow-y-auto">
          {nodes.map((node) => (
            <NodeRow
              key={node.key}
              node={node}
              expanded={expanded.has(node.key)}
              onToggle={() =>
                setExpanded((prev) => {
                  const next = new Set(prev);
                  next.has(node.key) ? next.delete(node.key) : next.add(node.key);
                  return next;
                })
              }
              selection={active}
              onSelectLeaf={(leaf) =>
                setSelection({ jobId: node.job.id, leafKey: leaf.key, kind: leaf.kind })
              }
            />
          ))}
        </div>
      </div>

      {/* Right: output */}
      <div className="bg-surface border border-border rounded-xl flex flex-col min-h-[28rem] max-h-[36rem]">
        <div className="px-4 py-3 border-b border-border flex items-center gap-3 shrink-0">
          <h3 className="text-sm font-semibold text-secondary truncate">
            {selectedJob ? `${selectedJob.stage_name} · ${active?.kind}` : 'Output'}
          </h3>
          {selectedJob?.state === 'failed' && onRetryJob && (
            <button
              onClick={() => onRetryJob(selectedJob)}
              className="px-3 py-1 text-xs bg-warning-soft text-warning border border-warning/30 rounded-lg hover:opacity-80 transition-colors focus:outline-none focus:ring-2 focus:ring-warning"
            >
              Retry stage
            </button>
          )}
          {/* Selected step status + exit code, pinned to the right. */}
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
              Select a step on the left
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
