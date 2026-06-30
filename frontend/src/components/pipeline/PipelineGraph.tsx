import { useMemo, useState } from 'react';
import { clsx } from 'clsx';
import { StatusBadge } from '../ui/StatusBadge';
import {
  clusterRows,
  leafToStep,
  type PipelineModel,
  type StepRef,
} from '../../lib/pipeline';

interface Props {
  model: PipelineModel;
  selectedKey?: string;
  onSelect: (step: StepRef) => void;
}

const LANE_W = 22; // px per lane in the graph gutter
const DOT_CY = 18; // px — vertical centre of a node's dot (aligns with header)

interface RailItem {
  key: string;
  title: string;
  state: string;
  /** Lane (column) this node sits on; 0 = main spine. */
  lane: number;
  steps: StepRef[];
}

/** dot/branch colour by node state. */
function dotColor(state: string): string {
  switch (state) {
    case 'success': return 'var(--color-success)';
    case 'failed': return 'var(--color-danger)';
    case 'cancelled':
    case 'expired': return 'var(--color-warning)';
    case 'running':
    case 'assigned': return 'var(--color-accent)';
    default: return 'var(--color-text-muted)';
  }
}

/** Per-row graph gutter: the continuous spine (lane 0), an elbow to this
 *  node's lane when it branches, and the node's dot. The SVG stretches to the
 *  row height so the spine stays connected even when a row is expanded. */
function Gutter({
  lane,
  maxLane,
  isFirst,
  isLast,
  color,
}: {
  lane: number;
  maxLane: number;
  isFirst: boolean;
  isLast: boolean;
  color: string;
}) {
  const width = (maxLane + 1) * LANE_W;
  const x0 = LANE_W / 2; // spine x
  const xL = lane * LANE_W + LANE_W / 2; // this node's x
  return (
    <svg width={width} className="shrink-0 h-full text-border-strong" preserveAspectRatio="none" aria-hidden="true">
      {/* main spine (lane 0) */}
      <line
        x1={x0}
        y1={isFirst ? DOT_CY : 0}
        x2={x0}
        y2={isLast && lane === 0 ? DOT_CY : '100%'}
        stroke="currentColor"
        strokeWidth={2}
      />
      {/* elbow from the spine out to a branch lane */}
      {lane > 0 && (
        <line x1={x0} y1={DOT_CY} x2={xL} y2={DOT_CY} stroke="currentColor" strokeWidth={2} />
      )}
      {/* node dot */}
      <circle cx={xL} cy={DOT_CY} r={4.5} fill={color} stroke="var(--color-surface)" strokeWidth={2} />
    </svg>
  );
}

function StepChip({ step, selected, onClick }: { step: StepRef; selected: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={clsx(
        'w-full flex items-center justify-between gap-2 pl-3 pr-2 py-1 text-left text-xs rounded-md transition-colors',
        'focus:outline-none focus:ring-2 focus:ring-accent',
        selected ? 'bg-accent-soft text-accent-text' : 'hover:bg-surface-hover/50 text-secondary',
      )}
    >
      <span className="font-mono truncate">{step.label}</span>
      <span className="flex items-center gap-1.5 shrink-0">
        {step.type === 'job' && step.exitCode !== null && (
          <span className={clsx('font-mono text-[10px]', step.exitCode === 0 ? 'text-disabled' : 'text-danger')}>
            {step.exitCode}
          </span>
        )}
        <StatusBadge status={step.state} />
      </span>
    </button>
  );
}

export function PipelineGraph({ model, selectedKey, onSelect }: Props) {
  const items = useMemo<RailItem[]>(() => {
    const out: RailItem[] = [];
    if (model.globalPre.present) {
      out.push({
        key: 'global-pre',
        title: 'Global pre-script',
        state: model.globalPre.steps[0]?.state ?? 'unknown',
        lane: 0,
        steps: model.globalPre.steps,
      });
    }
    for (const row of clusterRows(model.stages)) {
      row.forEach((node, i) => {
        out.push({
          key: node.key,
          title: node.stageName,
          state: node.state,
          lane: i, // node 0 stays on the spine; siblings branch off
          steps: node.leaves.map(leafToStep),
        });
      });
    }
    if (model.globalPost.present) {
      out.push({
        key: 'global-post',
        title: 'Global post-script',
        state: model.globalPost.steps[0]?.state ?? 'unknown',
        lane: 0,
        steps: model.globalPost.steps,
      });
    }
    return out;
  }, [model]);

  const maxLane = useMemo(() => items.reduce((m, it) => Math.max(m, it.lane), 0), [items]);

  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const toggle = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });

  return (
    <div className="flex flex-col">
      {items.map((it, i) => {
        const isOpen = expanded.has(it.key);
        return (
          <div key={it.key} className="flex items-stretch">
            <Gutter
              lane={it.lane}
              maxLane={maxLane}
              isFirst={i === 0}
              isLast={i === items.length - 1}
              color={dotColor(it.state)}
            />
            <div className="flex-1 min-w-0 pb-1">
              {/* node header: caret + name + status */}
              <button
                onClick={() => toggle(it.key)}
                className="w-full flex items-center gap-2 pr-1 text-left hover:bg-surface-hover/40 rounded-md transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
                style={{ minHeight: DOT_CY * 2 }}
                aria-expanded={isOpen}
              >
                <svg
                  className={clsx('w-3 h-3 text-disabled transition-transform shrink-0', isOpen && 'rotate-90')}
                  fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
                <span className="text-xs font-medium text-secondary font-mono truncate flex-1">{it.title}</span>
                <StatusBadge status={it.state} />
              </button>
              {/* expanded sub-steps as a small branch off the node */}
              {isOpen && it.steps.length > 0 && (
                <div className="ml-3 mt-0.5 mb-1 pl-2 border-l border-border/60 space-y-0.5">
                  {it.steps.map((s) => (
                    <StepChip key={s.key} step={s} selected={selectedKey === s.key} onClick={() => onSelect(s)} />
                  ))}
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
