import { clsx } from 'clsx';
import type { Job } from '../../types';
import { StatusBadge } from './StatusBadge';
import { formatDuration, durationMs } from '../../utils/duration';

function durationBar(start: string | null, end: string | null, maxMs: number): number {
  if (!start) return 0;
  return Math.min((durationMs(start, end) / maxMs) * 100, 100);
}

interface Props {
  jobs: Job[];
  onSelectJob: (job: Job) => void;
  selectedJobId?: string;
}

export function StageTimeline({ jobs, onSelectJob, selectedJobId }: Props) {
  const sorted = [...jobs].sort(
    (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime(),
  );
  const maxMs = sorted.reduce((max, j) => Math.max(max, durationMs(j.started_at, j.completed_at)), 1000);

  const stateColor: Record<string, string> = {
    success: 'bg-success',
    failed: 'bg-danger',
    running: 'bg-accent',
    cancelled: 'bg-warning',
    queued: 'bg-surface-2',
    assigned: 'bg-accent/70',
    unknown: 'bg-warning',
  };

  return (
    <div className="space-y-2" role="list" aria-label="Pipeline stages">
      {sorted.map((job, i) => (
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

          {/* Stage info */}
          <div className="w-40 shrink-0">
            <p className="text-sm font-medium text-secondary">{job.stage_name}</p>
            <p className="text-xs text-disabled">{formatDuration(job.started_at, job.completed_at)}</p>
          </div>

          {/* Duration bar */}
          <div
            className="flex-1 h-6 bg-surface-2 rounded overflow-hidden relative"
            role="progressbar"
            aria-label={`Duration: ${formatDuration(job.started_at, job.completed_at)}`}
            aria-valuenow={durationBar(job.started_at, job.completed_at, maxMs)}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div
              className={clsx(
                'h-full rounded transition-all duration-500',
                stateColor[job.state] ?? 'bg-surface-2',
              )}
              style={{ width: `${durationBar(job.started_at, job.completed_at, maxMs)}%` }}
            />
            {job.state === 'running' && (
              <div className="absolute inset-0 bg-accent/20 animate-pulse rounded" aria-hidden="true" />
            )}
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
      ))}
    </div>
  );
}
