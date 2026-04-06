import { clsx } from 'clsx';

type Status =
  | 'pending' | 'reserved' | 'queued' | 'assigned'
  | 'running'
  | 'success'
  | 'failed'
  | 'cancelled'
  | 'expired'
  | 'unknown'
  | 'Connected' | 'Disconnected' | 'Draining';

const statusStyles: Record<string, string> = {
  pending: 'bg-gray-500/20 text-gray-400 border-gray-500/30',
  reserved: 'bg-purple-500/20 text-purple-400 border-purple-500/30',
  queued: 'bg-gray-500/20 text-gray-400 border-gray-500/30',
  assigned: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  running: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  success: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  failed: 'bg-red-500/20 text-red-400 border-red-500/30',
  cancelled: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  expired: 'bg-amber-500/10 text-amber-400 border-amber-500/30',
  unknown: 'bg-orange-500/20 text-orange-400 border-orange-500/30',
  Connected: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  Disconnected: 'bg-red-500/20 text-red-400 border-red-500/30',
  Draining: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
};

const pulseStatuses = new Set(['running', 'assigned']);

interface Props {
  status: Status | string;
  size?: 'sm' | 'md';
}

export function StatusBadge({ status, size = 'sm' }: Props) {
  const style = statusStyles[status] ?? 'bg-gray-500/20 text-gray-400 border-gray-500/30';
  const pulse = pulseStatuses.has(status);

  return (
    <span
      className={clsx(
        'inline-flex items-center gap-1.5 border rounded-full font-medium',
        style,
        size === 'sm' ? 'px-2 py-0.5 text-xs' : 'px-3 py-1 text-sm',
      )}
      aria-label={`Status: ${status}`}
    >
      {pulse && (
        <span className="relative flex h-2 w-2" aria-hidden="true">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75" />
          <span className="relative inline-flex rounded-full h-2 w-2 bg-blue-500" />
        </span>
      )}
      {status}
    </span>
  );
}
