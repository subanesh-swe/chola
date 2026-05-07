import { clsx } from 'clsx';

type Status =
  | 'pending' | 'reserved' | 'queued' | 'assigned'
  | 'running'
  | 'success'
  | 'failed'
  | 'cancelled'
  | 'expired'
  | 'unknown'
  | 'archived'
  | 'files-purged'
  | 'Connected' | 'Disconnected' | 'Draining';

const statusStyles: Record<string, string> = {
  pending: 'bg-surface-2/50 text-muted border-border',
  reserved: 'bg-purple-500/20 text-purple-400 border-purple-500/30',
  queued: 'bg-surface-2/50 text-muted border-border',
  assigned: 'bg-accent-soft text-accent-text border-accent/30',
  running: 'bg-accent-soft text-accent-text border-accent/30',
  success: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  failed: 'bg-red-500/20 text-red-400 border-red-500/30',
  cancelled: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  expired: 'bg-amber-500/10 text-amber-400 border-amber-500/30',
  unknown: 'bg-orange-500/20 text-orange-400 border-orange-500/30',
  archived: 'bg-slate-500/20 text-slate-400 border-slate-500/30',
  'files-purged': 'bg-slate-600/20 text-slate-500 border-slate-600/30',
  Connected: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  Disconnected: 'bg-red-500/20 text-red-400 border-red-500/30',
  Draining: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
};

/** Map raw enum values to human-readable labels. */
const labelMap: Record<string, string> = {
  // JOB_STATE_*
  JOB_STATE_UNSPECIFIED: 'Unknown',
  JOB_STATE_QUEUED: 'Queued',
  JOB_STATE_ASSIGNED: 'Assigned',
  JOB_STATE_RUNNING: 'Running',
  JOB_STATE_SUCCESS: 'Success',
  JOB_STATE_FAILED: 'Failed',
  JOB_STATE_CANCELLED: 'Cancelled',
  JOB_STATE_UNKNOWN: 'Unknown',
  // JOB_GROUP_STATE_*
  JOB_GROUP_STATE_UNSPECIFIED: 'Unknown',
  JOB_GROUP_STATE_PENDING: 'Pending',
  JOB_GROUP_STATE_RESERVED: 'Reserved',
  JOB_GROUP_STATE_RUNNING: 'Running',
  JOB_GROUP_STATE_SUCCESS: 'Success',
  JOB_GROUP_STATE_FAILED: 'Failed',
  JOB_GROUP_STATE_CANCELLED: 'Cancelled',
  JOB_GROUP_STATE_EXPIRED: 'Expired',
  // WORKER_STATE_*
  WORKER_STATE_UNSPECIFIED: 'Unknown',
  WORKER_STATE_CONNECTED: 'Connected',
  WORKER_STATE_DISCONNECTED: 'Disconnected',
  WORKER_STATE_DRAINING: 'Draining',
};

/** Styles keyed to normalised canonical values (lower-case). */
const enumStyleMap: Record<string, string> = {
  unknown: 'bg-orange-500/20 text-orange-400 border-orange-500/30',
  queued: 'bg-surface-2/50 text-muted border-border',
  assigned: 'bg-accent-soft text-accent-text border-accent/30',
  running: 'bg-accent-soft text-accent-text border-accent/30',
  pending: 'bg-surface-2/50 text-muted border-border',
  reserved: 'bg-purple-500/20 text-purple-400 border-purple-500/30',
  success: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  failed: 'bg-red-500/20 text-red-400 border-red-500/30',
  cancelled: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  expired: 'bg-amber-500/10 text-amber-400 border-amber-500/30',
  connected: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  disconnected: 'bg-red-500/20 text-red-400 border-red-500/30',
  draining: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  // repo enabled/disabled (item 31)
  enabled: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  disabled: 'bg-surface-2 text-muted border-border',
};

const pulseStatuses = new Set(['running', 'assigned', 'JOB_STATE_RUNNING', 'JOB_STATE_ASSIGNED', 'JOB_GROUP_STATE_RUNNING']);

/** Fallback: "JOB_STATE_RUNNING" -> "Running", "some_thing" -> "Some thing". */
function prettify(raw: string): string {
  // Strip known prefixes
  const stripped = raw
    .replace(/^JOB_GROUP_STATE_/, '')
    .replace(/^JOB_STATE_/, '')
    .replace(/^WORKER_STATE_/, '');
  const lower = stripped.toLowerCase().replace(/_/g, ' ');
  return lower.charAt(0).toUpperCase() + lower.slice(1);
}

interface Props {
  status: Status | string;
  size?: 'sm' | 'md';
  /** Optional tooltip text shown on hover. */
  title?: string;
}

export function StatusBadge({ status, size = 'sm', title }: Props) {
  const label = labelMap[status] ?? prettify(status);
  const canonical = label.toLowerCase();

  const style =
    statusStyles[status] ??
    enumStyleMap[canonical] ??
    'bg-surface-2/50 text-muted border-border';

  const pulse = pulseStatuses.has(status);

  return (
    <span
      className={clsx(
        'inline-flex items-center gap-1.5 border rounded-full font-medium',
        style,
        size === 'sm' ? 'px-2 py-0.5 text-xs' : 'px-3 py-1 text-sm',
      )}
      aria-label={`Status: ${label}`}
      title={title}
    >
      {pulse && (
        <span className="relative flex h-2 w-2" aria-hidden="true">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-accent opacity-75" />
          <span className="relative inline-flex rounded-full h-2 w-2 bg-accent" />
        </span>
      )}
      {label}
    </span>
  );
}
