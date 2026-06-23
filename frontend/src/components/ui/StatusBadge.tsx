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
  reserved: 'bg-pending-soft text-pending border-pending/30',
  queued: 'bg-surface-2/50 text-muted border-border',
  assigned: 'bg-accent-soft text-accent-text border-accent/30',
  running: 'bg-accent-soft text-accent-text border-accent/30',
  success: 'bg-success-soft text-success border-success/30',
  failed: 'bg-danger-soft text-danger border-danger/30',
  cancelled: 'bg-warning-soft text-warning border-warning/30',
  expired: 'bg-warning-soft text-warning border-warning/30',
  unknown: 'bg-info-soft text-info border-info/30',
  archived: 'bg-surface-2/60 text-muted border-border',
  'files-purged': 'bg-surface-2/40 text-disabled border-border',
  Connected: 'bg-success-soft text-success border-success/30',
  Disconnected: 'bg-danger-soft text-danger border-danger/30',
  Draining: 'bg-warning-soft text-warning border-warning/30',
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
  unknown: 'bg-info-soft text-info border-info/30',
  queued: 'bg-surface-2/50 text-muted border-border',
  assigned: 'bg-accent-soft text-accent-text border-accent/30',
  running: 'bg-accent-soft text-accent-text border-accent/30',
  pending: 'bg-surface-2/50 text-muted border-border',
  reserved: 'bg-pending-soft text-pending border-pending/30',
  success: 'bg-success-soft text-success border-success/30',
  failed: 'bg-danger-soft text-danger border-danger/30',
  cancelled: 'bg-warning-soft text-warning border-warning/30',
  expired: 'bg-warning-soft text-warning border-warning/30',
  connected: 'bg-success-soft text-success border-success/30',
  disconnected: 'bg-danger-soft text-danger border-danger/30',
  draining: 'bg-warning-soft text-warning border-warning/30',
  enabled: 'bg-success-soft text-success border-success/30',
  disabled: 'bg-surface-2 text-muted border-border',
};

// `running` shows a spinning loader — far more visible than a pulse-dot
// when scanning the page for the active stage. `assigned` (worker accepted
// but not yet started) keeps the pulse-dot.
const spinStatuses = new Set(['running', 'JOB_STATE_RUNNING', 'JOB_GROUP_STATE_RUNNING']);
const pulseStatuses = new Set(['assigned', 'JOB_STATE_ASSIGNED']);

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
  const spin = spinStatuses.has(status);

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
      {spin && (
        <svg
          className={clsx('animate-spin text-current', size === 'sm' ? 'h-3 w-3' : 'h-3.5 w-3.5')}
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="10" stroke="currentColor" strokeOpacity="0.25" strokeWidth="4" />
          <path d="M22 12a10 10 0 0 1-10 10" stroke="currentColor" strokeWidth="4" strokeLinecap="round" />
        </svg>
      )}
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
