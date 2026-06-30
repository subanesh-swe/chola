import { useMemo, useState } from 'react';
import { clsx } from 'clsx';
import { StatusBadge } from '../ui/StatusBadge';
import {
  clusterRows,
  leafToStep,
  type PipelineModel,
  type PipelineNode,
  type StepRef,
} from '../../lib/pipeline';

interface Props {
  model: PipelineModel;
  selectedKey?: string;
  onSelect: (step: StepRef) => void;
}

/** Horizontal centre (0..1) of lane `i` in a row of `count` lanes. */
function laneX(i: number, count: number): number {
  return (i + 0.5) / count;
}

/** Split/join connector band between `prev` lanes (top) and `cur` (bottom). */
function Connector({ prev, cur }: { prev: number; cur: number }) {
  const lines: { x1: number; x2: number }[] = [];
  const mx = 50;
  for (let i = 0; i < prev; i++) lines.push({ x1: laneX(i, prev) * 100, x2: mx });
  const top = lines.map((l, i) => ({ ...l, y1: 0, y2: 50, k: `t${i}` }));
  const bottom: { x1: number; y1: number; x2: number; y2: number; k: string }[] = [];
  for (let j = 0; j < cur; j++) bottom.push({ x1: mx, y1: 50, x2: laneX(j, cur) * 100, y2: 100, k: `b${j}` });
  const all = [...top.map((l) => ({ x1: l.x1, y1: l.y1, x2: l.x2, y2: l.y2, k: l.k })), ...bottom];
  return (
    <svg className="w-full h-5 text-border-strong" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
      {all.map((l) => (
        <line key={l.k} x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2} stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      ))}
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

function ExpandableNode({
  title,
  state,
  expanded,
  onToggle,
  children,
}: {
  title: string;
  state: string;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="w-full rounded-lg border border-border/60 bg-surface-2/40 overflow-hidden">
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-2 px-2.5 py-2 text-left hover:bg-surface-hover/50 transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
        aria-expanded={expanded}
      >
        <svg
          className={clsx('w-3.5 h-3.5 text-disabled transition-transform shrink-0', expanded && 'rotate-90')}
          fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
        <span className="text-xs font-medium text-secondary font-mono truncate flex-1">{title}</span>
        <StatusBadge status={state} />
      </button>
      {expanded && <div className="px-1.5 pb-1.5 pt-0.5 space-y-1">{children}</div>}
    </div>
  );
}

type RailRow =
  | { kind: 'global-pre' }
  | { kind: 'global-post' }
  | { kind: 'stages'; lanes: PipelineNode[] };

export function PipelineGraph({ model, selectedKey, onSelect }: Props) {
  const rows = useMemo<RailRow[]>(() => {
    const out: RailRow[] = [];
    if (model.globalPre.present) out.push({ kind: 'global-pre' });
    for (const lanes of clusterRows(model.stages)) out.push({ kind: 'stages', lanes });
    if (model.globalPost.present) out.push({ kind: 'global-post' });
    return out;
  }, [model]);

  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const toggle = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });

  const laneCount = (r: RailRow) => (r.kind === 'stages' ? r.lanes.length : 1);

  return (
    <div className="flex flex-col items-start w-full">
      {rows.map((row, ri) => (
        <div key={ri} className="w-full">
          {ri > 0 && <Connector prev={laneCount(rows[ri - 1])} cur={laneCount(row)} />}

          {row.kind === 'global-pre' && (
            <ExpandableNode
              title="Global pre-script"
              state={model.globalPre.steps[0]?.state ?? 'unknown'}
              expanded={expanded.has('global-pre')}
              onToggle={() => toggle('global-pre')}
            >
              {model.globalPre.steps.map((s) => (
                <StepChip key={s.key} step={s} selected={selectedKey === s.key} onClick={() => onSelect(s)} />
              ))}
            </ExpandableNode>
          )}

          {row.kind === 'global-post' && (
            <ExpandableNode
              title="Global post-script"
              state={model.globalPost.steps[0]?.state ?? 'unknown'}
              expanded={expanded.has('global-post')}
              onToggle={() => toggle('global-post')}
            >
              {model.globalPost.steps.map((s) => (
                <StepChip key={s.key} step={s} selected={selectedKey === s.key} onClick={() => onSelect(s)} />
              ))}
            </ExpandableNode>
          )}

          {row.kind === 'stages' && (
            <div className="flex items-start justify-start gap-3">
              {row.lanes.map((node) => {
                const steps = node.leaves.map(leafToStep);
                return (
                  <div key={node.key} className="flex-1 min-w-[8rem] max-w-[16rem]">
                    <ExpandableNode
                      title={node.stageName}
                      state={node.state}
                      expanded={expanded.has(node.key)}
                      onToggle={() => {
                        toggle(node.key);
                        // selecting the stage also surfaces its command output
                        const cmd = steps.find((s) => s.type === 'job' && s.kind === 'cmd');
                        if (cmd && !expanded.has(node.key)) onSelect(cmd);
                      }}
                    >
                      {steps.map((s) => (
                        <StepChip key={s.key} step={s} selected={selectedKey === s.key} onClick={() => onSelect(s)} />
                      ))}
                    </ExpandableNode>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
