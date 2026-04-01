import { useQuery } from '@tanstack/react-query';
import { getSettings } from '../api/settings';
import { LoadingSkeleton } from '../components/ui';

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl p-5">
      <h3 className="text-sm font-semibold text-slate-400 uppercase tracking-wider mb-4">{title}</h3>
      <div className="space-y-3">{children}</div>
    </div>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between py-1 border-b border-slate-800 last:border-0">
      <span className="text-sm text-slate-400">{label}</span>
      <span className="text-sm text-white font-mono">{value}</span>
    </div>
  );
}

function BoolBadge({ value }: { value: boolean }) {
  return (
    <span className={`px-2 py-0.5 rounded text-xs font-medium ${value ? 'bg-emerald-500/20 text-emerald-400' : 'bg-slate-700 text-slate-400'}`}>
      {value ? 'enabled' : 'disabled'}
    </span>
  );
}

export default function SettingsPage() {
  const { data, isLoading, error } = useQuery({ queryKey: ['settings'], queryFn: getSettings });

  if (isLoading) return <LoadingSkeleton />;
  if (error || !data) return <div className="text-red-400">Failed to load settings.</div>;

  return (
    <div className="space-y-6 max-w-2xl">
      <h2 className="text-2xl font-bold text-white">System Settings</h2>
      <p className="text-sm text-slate-500">Read-only view of active controller configuration.</p>

      <Section title="Auth">
        <Row label="Auth enabled" value={<BoolBadge value={data.auth.enabled} />} />
        <Row label="JWT expiry" value={`${data.auth.jwt_expiry_secs}s (${Math.round(data.auth.jwt_expiry_secs / 3600)}h)`} />
      </Section>

      <Section title="Scheduling">
        <Row label="Strategy" value={data.scheduling.strategy} />
        <Row label="NVMe preference" value={<BoolBadge value={data.scheduling.nvme_preference} />} />
        <Row label="Branch affinity" value={<BoolBadge value={data.scheduling.branch_affinity} />} />
      </Section>

      <Section title="Workers">
        <Row label="Heartbeat interval" value={`${data.workers.heartbeat_interval_secs}s`} />
        <Row label="Heartbeat timeout" value={`${data.workers.heartbeat_timeout_secs}s`} />
        <Row label="Max reconnect attempts" value={data.workers.max_reconnect_attempts} />
        <Row label="Reservation timeout" value={`${data.workers.reservation_timeout_secs}s (${Math.round(data.workers.reservation_timeout_secs / 3600)}h)`} />
      </Section>

      <Section title="Logging">
        <Row label="Level" value={data.logging.level} />
        <Row label="Log directory" value={data.logging.log_dir ?? <span className="text-slate-500">not set</span>} />
      </Section>
    </div>
  );
}
