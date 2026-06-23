import { useState, useEffect } from 'react';
import { clsx } from 'clsx';
import type { Job } from '../../types';
import { StatusBadge } from './StatusBadge';
import { formatDuration } from '../../utils/duration';
import { formatSecs } from '../../utils/format';

/**
 * Bar fill fraction (0..1) for a stage, anchored to its OWN timeout budget
 * — never to sibling stages. This is the key difference from the old
 * relative-to-longest-sibling scaling, which made bars rescale (and appear
 * to shrink) as other stages ran longer.
 *
 *  - With a timeout: elapsed / max_duration_secs, capped at 1.0. The bar
 *    fills toward the deadline. A 20s stage on an 11m budget shows ~3%.
 *  - Without a timeout: always full (1.0) — there's no budget to
 *    proportion against.
 *
 * A small floor keeps a completed stage from rendering an invisible sliver.
 */
function barFraction(job: Job, now: number): number {
  if (!job.started_at) return 0;
  const elapsedMs = (job.completed_at ? new Date(job.completed_at).getTime() : now) - new Date(job.started_at).getTime();
  const elapsedSecs = Math.max(0, elapsedMs / 1000);
  const max = job.max_duration_secs;
  if (!max || max <= 0) return 1; // no budget → full bar
  const frac = Math.min(elapsedSecs / max, 1);
  return Math.max(frac, 0.02); // floor so it's always visible once started
}

/** "20s / 11m (3%)" for budgeted stages, "20s" when there's no timeout. */
function barLabel(job: Job, now: number): string {
  const elapsed = formatDuration(job.started_at, job.completed_at);
  const max = job.max_duration_secs;
  if (!max || max <= 0) return elapsed;
  const elapsedMs = (job.completed_at ? new Date(job.completed_at).getTime() : now) - new Date(job.started_at ?? now).getTime();
  const pct = Math.round(Math.min((elapsedMs / 1000) / max, 1) * 100);
  return `${elapsed} / ${formatSecs(max)} (${pct}%)`;
}

interface Props {
  jobs: Job[];
  onSelectJob: (job: Job) => void;
  selectedJobId?: string;
}

export function StageTimeline({ jobs, onSelectJob, selectedJobId }: Props) {
  // Tick every second while any stage is running so the toward-timeout
  // bars and countdown labels advance smoothly.
  const hasRunning = jobs.some((j) => j.state === 'running');
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!hasRunning) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [hasRunning]);

  const sorted = [...jobs].sort(
    (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime(),
  );

  // Fill color by state. Running uses the striped accent fill (handled
  // separately); terminal states get a solid status color.
  const stateColor: Record<string, string> = {
    success: 'bg-success',
    failed: 'bg-danger',
    cancelled: 'bg-warning',
    expired: 'bg-warning',
    queued: 'bg-surface-2',
    assigned: 'bg-accent/70',
    unknown: 'bg-warning',
  };

  return (
    <div className="space-y-2" role="list" aria-label="Pipeline stages">
      {sorted.map((job, i) => {
        const isRunning = job.state === 'running';
        const frac = barFraction(job, now);
        const label = barLabel(job, now);
        const hasTimeout = !!job.max_duration_secs && job.max_duration_secs > 0;
        // Remaining countdown for a running, budgeted stage.
        const remainingSecs =
          isRunning && hasTimeout && job.started_at
            ? Math.max(0, job.max_duration_secs - Math.floor((now - new Date(job.started_at).getTime()) / 1000))
            : null;

        return (
          <div
            key={job.id}
            role="listitem"
            onClick={() => onSelectJob(job)}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelectJob(job); } }}
            tabIndex={0}
            aria-label={`Stage ${i + 1}: ${job.stage_name}, ${job.state}`}
            aria-pressed={selectedJobId === job.id}
            className={clsx(
              'flex items-center gap-4 px-4 py-3 rounded-lg cursor-pointer transition-all',
              'focus:outline-none focus:ring-2 focus:ring-accent',
              selectedJobId === job.id
                ? 'bg-surface-2 ring-1 ring-accent/50'
                : 'hover:bg-surface-hover/50',
            )}
          >
            {/* Step number */}
            <div
              aria-hidden="true"
              className="w-8 h-8 rounded-full bg-surface-2 flex items-center justify-center text-xs font-bold text-secondary shrink-0"
            >
              {i + 1}
            </div>

            {/* Stage info: name + timeout-aware time line (per user request,
                the timeout shows next to the stage name, not in the header). */}
            <div className="w-48 shrink-0">
              <p className="text-sm font-medium text-secondary truncate">{job.stage_name}</p>
              <p className="text-xs text-disabled font-mono">
                {isRunning && remainingSecs != null
                  ? `${formatSecs(remainingSecs)} left / ${formatSecs(job.max_duration_secs)}`
                  : isRunning && !hasTimeout
                  ? `${formatDuration(job.started_at, null)} · no limit`
                  : label}
              </p>
            </div>

            {/* Duration / budget bar */}
            <div
              className="flex-1 h-6 bg-surface-2 rounded overflow-hidden relative"
              role="progressbar"
              aria-label={`Stage time: ${label}`}
              aria-valuenow={Math.round(frac * 100)}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-busy={isRunning}
              title={label}
            >
              <div
                className={clsx(
                  'h-full rounded transition-all ease-linear',
                  isRunning ? 'bg-accent/60 animate-stripes duration-1000' : (stateColor[job.state] ?? 'bg-surface-2'),
                  isRunning ? '' : 'duration-500',
                )}
                style={{ width: `${frac * 100}%` }}
              />
            </div>

            {/* Status */}
            <div className="w-28 shrink-0 text-right">
              <StatusBadge status={job.state} />
            </div>

            {/* Exit code */}
            <div className="w-16 text-right text-xs text-disabled font-mono" aria-label={job.exit_code !== null ? `Exit code ${job.exit_code}` : ''}>
              {job.exit_code !== null ? `exit ${job.exit_code}` : ''}
            </div>
          </div>
        );
      })}
    </div>
  );
}
