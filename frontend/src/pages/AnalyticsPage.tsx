import { useEffect, useRef } from 'react';
import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { getAnalytics } from '../api/analytics';
import { listRepos } from '../api/repos';
import { useAppliedFilters } from '../hooks/useAppliedFilters';
import { useRefreshInterval } from '../hooks/useRefreshInterval';
import { FilterBar } from '../components/ui/FilterBar';
import { RefreshControl } from '../components/ui/RefreshControl';
import type { SlowStage, FailingRepo, WorkerUtilization } from '../types';
import {
  AreaChart, Area, LineChart, Line, BarChart, Bar,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from 'recharts';

const COLORS = {
  success: '#10b981',
  failed: '#ef4444',
  duration: '#3b82f6',
  p95: '#f59e0b',
  grid: '#334155',
  text: '#94a3b8',
};

function fmtDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}

function activeDays(dateFrom: string): number | null {
  if (!dateFrom) return null;
  const diff = Math.round((Date.now() - new Date(dateFrom).getTime()) / 86400000);
  return diff;
}

function StatCard({ label, value, sub, color }: {
  label: string; value: string | number; sub?: string; color: string;
}) {
  return (
    <div className={`bg-surface border ${color} rounded-xl p-4`}>
      <p className="text-xs text-muted">{label}</p>
      <p className="text-2xl font-bold text-primary mt-0.5">{value}</p>
      {sub && <p className="text-xs text-disabled mt-0.5">{sub}</p>}
    </div>
  );
}

function ChartCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-surface border border-border rounded-xl">
      <div className="px-4 py-3 border-b border-border">
        <h3 className="text-sm font-semibold text-secondary">{title}</h3>
      </div>
      <div className="p-4">{children}</div>
    </div>
  );
}

function ChartTooltipContent({ active, payload, label }: {
  active?: boolean; payload?: Array<{ name: string; value: number; color: string }>;
  label?: string;
}) {
  if (!active || !payload?.length) return null;
  return (
    <div className="bg-surface-2 border border-border rounded-lg px-3 py-2 text-xs">
      <p className="text-muted mb-1">{label}</p>
      {payload.map((p) => (
        <p key={p.name} style={{ color: p.color }}>
          {p.name}: {p.value}
        </p>
      ))}
    </div>
  );
}

function HBarChart({ data, nameKey, valueKey, label, color }: {
  data: Array<Record<string, string | number>>;
  nameKey: string; valueKey: string; label: string; color: string;
}) {
  if (!data.length) {
    return <p className="text-disabled text-sm text-center py-6">No data</p>;
  }
  return (
    <ResponsiveContainer width="100%" height={Math.max(data.length * 36, 120)}>
      <BarChart data={data} layout="vertical" margin={{ left: 10, right: 20, top: 4, bottom: 4 }}>
        <CartesianGrid strokeDasharray="3 3" stroke={COLORS.grid} horizontal={false} />
        <XAxis type="number" tick={{ fill: COLORS.text, fontSize: 11 }} />
        <YAxis
          type="category"
          dataKey={nameKey}
          tick={{ fill: COLORS.text, fontSize: 11 }}
          width={120}
        />
        <Tooltip content={<ChartTooltipContent />} />
        <Bar dataKey={valueKey} name={label} fill={color} radius={[0, 4, 4, 0]} barSize={20} />
      </BarChart>
    </ResponsiveContainer>
  );
}

function SlowestStagesChart({ data }: { data: SlowStage[] }) {
  const items = data.map((s) => ({
    name: `${s.stage_name} (${s.repo_name})`,
    avg_secs: s.avg_secs,
  }));
  return <HBarChart data={items} nameKey="name" valueKey="avg_secs" label="Avg (s)" color={COLORS.p95} />;
}

function FailingReposChart({ data }: { data: FailingRepo[] }) {
  const items = data.map((r) => ({ name: r.repo_name, failed: r.failed }));
  return <HBarChart data={items} nameKey="name" valueKey="failed" label="Failed" color={COLORS.failed} />;
}

function WorkerUtilChart({ data }: { data: WorkerUtilization[] }) {
  if (!data.length) {
    return <p className="text-disabled text-sm text-center py-6">No workers</p>;
  }
  const items = data.map((w) => ({
    name: w.hostname || w.worker_id.slice(0, 12),
    active: w.active_jobs,
    total_30d: w.total_jobs_30d,
  }));
  return (
    <ResponsiveContainer width="100%" height={Math.max(items.length * 36, 120)}>
      <BarChart data={items} layout="vertical" margin={{ left: 10, right: 20, top: 4, bottom: 4 }}>
        <CartesianGrid strokeDasharray="3 3" stroke={COLORS.grid} horizontal={false} />
        <XAxis type="number" tick={{ fill: COLORS.text, fontSize: 11 }} />
        <YAxis type="category" dataKey="name" tick={{ fill: COLORS.text, fontSize: 11 }} width={120} />
        <Tooltip content={<ChartTooltipContent />} />
        <Bar dataKey="active" name="Active" fill={COLORS.success} radius={[0, 4, 4, 0]} barSize={16} />
        <Bar dataKey="total_30d" name="30d total" fill={COLORS.duration} radius={[0, 4, 4, 0]} barSize={16} />
      </BarChart>
    </ResponsiveContainer>
  );
}

export default function AnalyticsPage() {
  const { applied, draft, patchDraft, apply, applyPatch, reset, isDirty } = useAppliedFilters();
  const [refreshSecs, setRefreshSecs] = useRefreshInterval('analytics', 30);

  const { data: reposData } = useQuery({
    queryKey: ['repos'],
    queryFn: () => listRepos({ limit: 100 }),
  });
  const repos = reposData?.data ?? [];

  // Query is keyed on `applied` so it only refetches after the user clicks Search
  // or uses a preset (which calls applyPatch directly).
  const appliedKey = JSON.stringify(applied);
  const { data, isLoading, isError, isFetching, refetch } = useQuery({
    queryKey: ['analytics', appliedKey],
    queryFn: () => getAnalytics(applied),
    placeholderData: keepPreviousData,
    refetchInterval: refreshSecs > 0 ? refreshSecs * 1000 : false,
  });

  // Keep a stable ref to refetch so it can be called from event handlers without
  // re-creating them on every render.
  const refetchRef = useRef(refetch);
  useEffect(() => {
    refetchRef.current = refetch;
  }, [refetch]);

  const days = activeDays(applied.dateFrom);

  const handlePresetApply = (patch: Partial<typeof applied>) => {
    applyPatch(patch);
  };

  if (isError) {
    return (
      <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
        <h3 className="font-semibold">Failed to load analytics</h3>
        <p className="text-sm mt-1">An error occurred. Please try again.</p>
        <button
          onClick={() => void refetchRef.current()}
          className="mt-3 px-3 py-1 bg-red-800 hover:bg-red-700 rounded text-sm text-white"
        >
          Retry
        </button>
      </div>
    );
  }

  if (isLoading || !data) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-8 w-48 bg-surface-2 rounded" />
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          {[...Array(4)].map((_, i) => <div key={i} className="h-24 bg-surface-2 rounded-xl" />)}
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {[...Array(4)].map((_, i) => <div key={i} className="h-64 bg-surface-2 rounded-xl" />)}
        </div>
      </div>
    );
  }

  const { summary, build_trends, duration_trends, slowest_stages, failing_repos, worker_utilization, queue_wait_trends } = data;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-primary">Analytics</h2>
        <RefreshControl
          intervalSecs={refreshSecs}
          onIntervalChange={setRefreshSecs}
          onRefresh={() => void refetchRef.current()}
          isFetching={isFetching}
        />
      </div>

      <FilterBar
        filters={draft}
        repos={repos}
        onChange={patchDraft}
        onApply={apply}
        onReset={reset}
        isDirty={isDirty}
        isFetching={isFetching}
        onPresetApply={handlePresetApply}
      />

      {/* Summary cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard
          label="Total Builds"
          value={summary.total_builds}
          sub={days !== null ? `Last ${days} days` : 'All time'}
          color="border-accent/30"
        />
        <StatCard
          label="Success Rate"
          value={`${summary.success_rate}%`}
          sub={`${summary.total_builds > 0 ? Math.round(summary.total_builds * summary.success_rate / 100) : 0} succeeded`}
          color={summary.success_rate >= 80 ? 'border-emerald-500/30' : 'border-yellow-500/30'}
        />
        <StatCard
          label="Avg Duration"
          value={fmtDuration(summary.avg_duration_secs)}
          color="border-accent/30"
        />
        <StatCard
          label="Avg Queue Wait"
          value={fmtDuration(summary.avg_queue_wait_secs)}
          color="border-border"
        />
      </div>

      {/* Build trends + Duration trends */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <ChartCard title="Build Trends">
          {build_trends.length ? (
            <ResponsiveContainer width="100%" height={240}>
              <AreaChart data={build_trends} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={COLORS.grid} />
                <XAxis dataKey="date" tick={{ fill: COLORS.text, fontSize: 11 }} />
                <YAxis tick={{ fill: COLORS.text, fontSize: 11 }} />
                <Tooltip content={<ChartTooltipContent />} />
                <Area type="monotone" dataKey="success" name="Success" stackId="1"
                  stroke={COLORS.success} fill={COLORS.success} fillOpacity={0.3} />
                <Area type="monotone" dataKey="failed" name="Failed" stackId="1"
                  stroke={COLORS.failed} fill={COLORS.failed} fillOpacity={0.3} />
              </AreaChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-disabled text-sm text-center py-16">No build data</p>
          )}
        </ChartCard>

        <ChartCard title="Duration Trends">
          {duration_trends.length ? (
            <ResponsiveContainer width="100%" height={240}>
              <LineChart data={duration_trends} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={COLORS.grid} />
                <XAxis dataKey="date" tick={{ fill: COLORS.text, fontSize: 11 }} />
                <YAxis tick={{ fill: COLORS.text, fontSize: 11 }} />
                <Tooltip content={<ChartTooltipContent />} />
                <Line type="monotone" dataKey="avg_duration_secs" name="Avg (s)"
                  stroke={COLORS.duration} strokeWidth={2} dot={false} />
                <Line type="monotone" dataKey="p95_duration_secs" name="p95 (s)"
                  stroke={COLORS.p95} strokeWidth={2} dot={false} strokeDasharray="5 5" />
              </LineChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-disabled text-sm text-center py-16">No duration data</p>
          )}
        </ChartCard>
      </div>

      {/* Slowest stages + Failing repos */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <ChartCard title="Slowest Stages">
          <SlowestStagesChart data={slowest_stages} />
        </ChartCard>
        <ChartCard title={`Most Failing Repos${days !== null ? ` (${days}d)` : ''}`}>
          <FailingReposChart data={failing_repos} />
        </ChartCard>
      </div>

      {/* Worker utilization + Queue wait */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <ChartCard title="Worker Utilization">
          <WorkerUtilChart data={worker_utilization} />
        </ChartCard>
        <ChartCard title="Queue Wait Time">
          {queue_wait_trends.length ? (
            <ResponsiveContainer width="100%" height={240}>
              <AreaChart data={queue_wait_trends} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={COLORS.grid} />
                <XAxis dataKey="date" tick={{ fill: COLORS.text, fontSize: 11 }} />
                <YAxis tick={{ fill: COLORS.text, fontSize: 11 }} />
                <Tooltip content={<ChartTooltipContent />} />
                <Area type="monotone" dataKey="avg_wait_secs" name="Avg wait (s)"
                  stroke={COLORS.duration} fill={COLORS.duration} fillOpacity={0.2} />
              </AreaChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-disabled text-sm text-center py-16">No queue data</p>
          )}
        </ChartCard>
      </div>
    </div>
  );
}
