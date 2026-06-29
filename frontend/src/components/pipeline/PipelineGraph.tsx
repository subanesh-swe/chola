import { useMemo } from 'react';
import { clsx } from 'clsx';
import { StatusBadge } from '../ui/StatusBadge';
import { clusterRows, type PipelineNode } from '../../lib/pipeline';

interface Props {
  nodes: PipelineNode[];
  selectedJobId?: string;
  onSelectNode: (node: PipelineNode) => void;
}

/** Horizontal centre (0..1) of lane `i` in a row of `count` lanes. */
function laneX(i: number, count: number): number {
  return (i + 0.5) / count;
}

/**
 * Connector band between a row of `prev` lanes (top) and `cur` lanes
 * (bottom). Lines fan from each top centre into a shared midpoint, then out
 * to each bottom centre — reads as a join-then-split (git-graph style).
 */
function Connector({ prev, cur }: { prev: number; cur: number }) {
  const lines: { x1: number; y1: number; x2: number; y2: number }[] = [];
  const mx = 50;
  for (let i = 0; i < prev; i++) lines.push({ x1: laneX(i, prev) * 100, y1: 0, x2: mx, y2: 50 });
  for (let j = 0; j < cur; j++) lines.push({ x1: mx, y1: 50, x2: laneX(j, cur) * 100, y2: 100 });
  return (
    <svg
      className="w-full h-6 text-border-strong"
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      {lines.map((l, i) => (
        <line key={i} x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2} stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      ))}
    </svg>
  );
}

function StageNode({
  node,
  selected,
  onSelect,
}: {
  node: PipelineNode;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      onClick={onSelect}
      className={clsx(
        'flex flex-col items-center gap-1 px-3 py-2 rounded-lg border transition-colors min-w-[7rem] max-w-[12rem]',
        'focus:outline-none focus:ring-2 focus:ring-accent',
        selected
          ? 'bg-accent-soft border-accent/50'
          : 'bg-surface-2/40 border-border/60 hover:bg-surface-hover/50',
      )}
      title={node.stageName}
    >
      <span className="text-xs font-medium text-secondary font-mono truncate w-full text-center">
        {node.stageName}
      </span>
      <StatusBadge status={node.state} />
    </button>
  );
}

export function PipelineGraph({ nodes, selectedJobId, onSelectNode }: Props) {
  const rows = useMemo(() => clusterRows(nodes), [nodes]);

  return (
    <div className="flex flex-col items-stretch">
      {rows.map((row, ri) => (
        <div key={ri}>
          {ri > 0 && <Connector prev={rows[ri - 1].length} cur={row.length} />}
          <div className="flex items-stretch justify-center gap-3">
            {row.map((node) => (
              <StageNode
                key={node.key}
                node={node}
                selected={node.job.id === selectedJobId}
                onSelect={() => onSelectNode(node)}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
