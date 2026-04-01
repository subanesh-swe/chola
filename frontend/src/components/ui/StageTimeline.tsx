import { clsx } from 'clsx';
import type { Job } from '../../types';
import { StatusBadge } from './StatusBadge';

function duration(start: string | null, end: string | null): string {
  if (!start) return '-';
  const ms = (end ? new Date(end).getTime() : Date.now()) - new Date(start).getTime();
  const secs = Math.round(ms / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

function durationBar(start: string | null, end: string | null, maxMs: number): number {
  if (!start) return 0;
  const ms = (end ? new Date(end).getTime() : Date.now()) - new Date(start).getTime();
  return Math.min((ms / maxMs) * 100, 100);
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
  const maxMs = sorted.reduce((max, j) => {
    if (!j.started_at) return max;
    const ms =
      (j.completed_at ? new Date(j.completed_at).getTime() : Date.now()) -
      new Date(j.started_at).getTime();
    return Math.max(max, ms);
  }, 1000);

  const stateColor: Record<string, string> = {
    success: 'bg-emerald-500',
    failed: 'bg-red-500',
    running: 'bg-blue-500',
    cancelled: 'bg-yellow-500',
    queued: 'bg-gray-600',
    assigned: 'bg-blue-400',
    unknown: 'bg-orange-500',
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
            'focus:outline-none focus:ring-2 focus:ring-blue-500',
            selectedJobId === job.id
              ? 'bg-slate-800 ring-1 ring-blue-500/50'
              : 'hover:bg-slate-800/50',
          )}
        >
          {/* Step number */}
          <div
            aria-hidden="true"
            className="w-8 h-8 rounded-full bg-slate-700 flex items-center justify-center text-xs font-bold text-slate-300 shrink-0"
          >
            {i + 1}
          </div>

          {/* Stage info */}
          <div className="w-40 shrink-0">
            <p className="text-sm font-medium text-slate-200">{job.stage_name}</p>
            <p className="text-xs text-slate-500">{duration(job.started_at, job.completed_at)}</p>
          </div>

          {/* Duration bar */}
          <div
            className="flex-1 h-6 bg-slate-800 rounded overflow-hidden relative"
            role="progressbar"
            aria-label={`Duration: ${duration(job.started_at, job.completed_at)}`}
            aria-valuenow={durationBar(job.started_at, job.completed_at, maxMs)}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div
              className={clsx(
                'h-full rounded transition-all duration-500',
                stateColor[job.state] ?? 'bg-gray-600',
              )}
              style={{ width: `${durationBar(job.started_at, job.completed_at, maxMs)}%` }}
            />
            {job.state === 'running' && (
              <div className="absolute inset-0 bg-blue-500/20 animate-pulse rounded" aria-hidden="true" />
            )}
          </div>

          {/* Status */}
          <div className="w-28 shrink-0 text-right">
            <StatusBadge status={job.state} />
          </div>

          {/* Exit code */}
          <div className="w-16 text-right text-xs text-slate-500 font-mono" aria-label={job.exit_code !== null ? `Exit code ${job.exit_code}` : ''}>
            {job.exit_code !== null ? `exit ${job.exit_code}` : ''}
          </div>
        </div>
      ))}
    </div>
  );
}
