import { useQuery } from '@tanstack/react-query';
import { getDashboardSummary } from '../api/dashboard';
import { listBuildsRaw } from '../api/builds';
import { listWorkers } from '../api/workers';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { Sparkline } from '../components/ui/Sparkline';
import { DashboardSkeleton } from '../components/ui/PageSkeleton';
import { ResourceBar } from '../components/ui/ResourceBar';
import { useNavigate } from 'react-router-dom';
import { useRef } from 'react';

// Accumulate a rolling window of numeric samples keyed by metric name
function useTrendWindow(value: number, key: string, windowSize = 12): number[] {
  const historyRef = useRef<Record<string, number[]>>({});
  if (!historyRef.current[key]) historyRef.current[key] = [];
  const arr = historyRef.current[key];
  if (arr.length === 0 || arr[arr.length - 1] !== value) {
    arr.push(value);
    if (arr.length > windowSize) arr.shift();
  }
  return [...arr];
}

function StatCardWithSparkline({
  label,
  value,
  trend,
  color,
  sparkColor,
  subtext,
}: {
  label: string;
  value: number | string;
  trend: number[];
  color: string;
  sparkColor: string;
  subtext?: string;
}) {
  return (
    <div className={`bg-slate-900 border ${color} rounded-xl p-4 flex flex-col gap-2`}>
      <div className="flex items-start justify-between">
        <div>
          <p className="text-xs text-slate-400">{label}</p>
          <p className="text-2xl font-bold text-white mt-0.5">{value}</p>
          {subtext && <p className="text-xs text-slate-500 mt-0.5">{subtext}</p>}
        </div>
        <div className="opacity-70">
          <Sparkline data={trend} color={sparkColor} width={80} height={28} />
        </div>
      </div>
    </div>
  );
}

function SystemStatusPanel({
  workers,
  summary,
}: {
  workers: { connected: number; total: number; draining: number };
  summary: { running: number; failed: number } | null;
}) {
  const items = [
    { label: 'Controller', ok: true },
    { label: 'Database', ok: true },
    { label: 'Workers Online', ok: workers.connected > 0, detail: `${workers.connected}/${workers.total}` },
    { label: 'Active Builds', ok: true, detail: summary?.running ?? 0 },
    { label: 'Failed (24h)', ok: (summary?.failed ?? 0) === 0, detail: summary?.failed ?? 0 },
  ];

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl">
      <div className="px-4 py-3 border-b border-slate-700">
        <h3 className="text-sm font-semibold text-slate-300">System Status</h3>
      </div>
      <div className="divide-y divide-slate-800">
        {items.map((item) => (
          <div key={item.label} className="px-4 py-2.5 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className={`w-2 h-2 rounded-full ${item.ok ? 'bg-emerald-500' : 'bg-red-500'}`} />
              <span className="text-sm text-slate-300">{item.label}</span>
            </div>
            {item.detail !== undefined && (
              <span className="text-xs text-slate-500 font-mono">{item.detail}</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function DashboardPage() {
  const nav = useNavigate();
  const { data: summary, isLoading: sumLoading, isError: sumError } = useQuery({
    queryKey: ['dashboard'],
    queryFn: getDashboardSummary,
    refetchInterval: 5000,
  });
  const { data: builds, isLoading: buildsLoading, isError: buildsError } = useQuery({
    queryKey: ['builds', 'recent'],
    queryFn: () => listBuildsRaw({ limit: 10 }),
    refetchInterval: 5000,
  });
  const { data: workersData, isLoading: workersLoading, isError: workersError } = useQuery({
    queryKey: ['workers'],
    queryFn: () => listWorkers(),
    refetchInterval: 5000,
  });

  const isLoading = sumLoading || buildsLoading || workersLoading;
  const isError = sumError || buildsError || workersError;

  const wc = workersData?.data ?? [];
  const connected = summary?.workers.connected ?? wc.filter((w) => w.status === 'Connected').length;
  const draining = summary?.workers.draining ?? wc.filter((w) => w.status === 'Draining').length;
  const running = summary?.job_groups?.running ?? 0;
  const failed = summary?.job_groups?.failed ?? 0;
  const successCount = summary?.job_groups?.success ?? 0;
  const totalDone = successCount + failed;
  const successRate = totalDone > 0 ? Math.round((successCount / totalDone) * 100) : 100;

  // Sparkline trend history (grows each render cycle as data refreshes)
  const activeTrend = useTrendWindow(running, 'active');
  const workerTrend = useTrendWindow(connected, 'workers');
  const rateTrend = useTrendWindow(successRate, 'rate');
  const failedTrend = useTrendWindow(failed, 'failed');

  if (isLoading) return <DashboardSkeleton />;
  if (isError) return (
    <div className="p-6">
      <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
        <h3 className="font-semibold">Failed to load dashboard</h3>
        <p className="text-sm mt-1">An error occurred. Please try again.</p>
        <button onClick={() => window.location.reload()} className="mt-3 px-3 py-1 bg-red-800 hover:bg-red-700 rounded text-sm text-white">Retry</button>
      </div>
    </div>
  );

  const recentBuilds = builds?.data ?? summary?.recent_builds ?? [];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">Dashboard</h2>
        <p className="text-xs text-slate-500">Auto-refreshes every 5s</p>
      </div>

      {/* Stat cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCardWithSparkline
          label="Active Builds"
          value={running}
          trend={activeTrend}
          color="border-blue-500/30"
          sparkColor="#3b82f6"
          subtext={`${summary?.job_groups?.pending ?? 0} pending`}
        />
        <StatCardWithSparkline
          label="Connected Workers"
          value={connected}
          trend={workerTrend}
          color="border-emerald-500/30"
          sparkColor="#10b981"
          subtext={draining > 0 ? `${draining} draining` : `${wc.length} total`}
        />
        <StatCardWithSparkline
          label="Success Rate"
          value={`${successRate}%`}
          trend={rateTrend}
          color={successRate >= 80 ? 'border-emerald-500/30' : 'border-yellow-500/30'}
          sparkColor={successRate >= 80 ? '#10b981' : '#f59e0b'}
          subtext={`${successCount} succeeded`}
        />
        <StatCardWithSparkline
          label="Failed Builds"
          value={failed}
          trend={failedTrend}
          color={failed > 0 ? 'border-red-500/30' : 'border-slate-700'}
          sparkColor="#ef4444"
          subtext={`${totalDone} total completed`}
        />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Recent Builds - status timeline */}
        <div className="lg:col-span-2 bg-slate-900 border border-slate-700 rounded-xl">
          <div className="px-4 py-3 border-b border-slate-700 flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-300">Recent Builds</h3>
            <button
              onClick={() => nav('/builds')}
              className="text-xs text-blue-400 hover:text-blue-300 transition-colors"
            >
              View all
            </button>
          </div>
          <div className="divide-y divide-slate-800">
            {(recentBuilds as Array<{ job_group_id: string; repo_name?: string; branch: string | null; state: string; created_at: string; commit_sha?: string | null }>).slice(0, 8).map((b) => (
              <div
                key={b.job_group_id}
                onClick={() => nav(`/builds/${b.job_group_id}`)}
                className="px-4 py-3 flex items-center gap-3 hover:bg-slate-800/50 cursor-pointer transition-colors group"
              >
                <StatusBadge status={b.state} />
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-slate-200 truncate">
                    {b.repo_name || b.job_group_id.slice(0, 8)}
                  </p>
                  <p className="text-xs text-slate-500 truncate">
                    {b.branch ?? 'no branch'}
                    {b.commit_sha ? ` @ ${b.commit_sha.slice(0, 7)}` : ''}
                  </p>
                </div>
                <div className="shrink-0 text-right">
                  <TimeAgo date={b.created_at} className="text-xs text-slate-500" />
                </div>
                <svg className="w-4 h-4 text-slate-600 group-hover:text-slate-400 transition-colors shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </div>
            ))}
            {!recentBuilds.length && (
              <div className="px-4 py-8 text-center text-slate-500">No recent builds</div>
            )}
          </div>
        </div>

        {/* Right column: worker health + system status */}
        <div className="flex flex-col gap-6">
          {/* Worker Health grid */}
          <div className="bg-slate-900 border border-slate-700 rounded-xl">
            <div className="px-4 py-3 border-b border-slate-700 flex items-center justify-between">
              <h3 className="text-sm font-semibold text-slate-300">Worker Health</h3>
              <button
                onClick={() => nav('/workers')}
                className="text-xs text-blue-400 hover:text-blue-300 transition-colors"
              >
                View all
              </button>
            </div>
            <div className="divide-y divide-slate-800">
              {wc.slice(0, 4).map((w) => (
                <div key={w.worker_id} className="px-4 py-3">
                  <div className="flex items-center justify-between mb-1.5">
                    <div className="flex items-center gap-2">
                      <span className={`w-1.5 h-1.5 rounded-full ${
                        w.status === 'Connected' ? 'bg-emerald-500' :
                        w.status === 'Draining' ? 'bg-yellow-500' : 'bg-red-500'
                      }`} />
                      <p className="text-xs text-slate-300 truncate max-w-[120px]">{w.hostname || w.worker_id}</p>
                    </div>
                    {w.last_heartbeat && (
                      <span className="text-xs text-slate-500">{w.last_heartbeat.running_jobs} job{w.last_heartbeat.running_jobs !== 1 ? 's' : ''}</span>
                    )}
                  </div>
                  {w.last_heartbeat && w.total_cpu > 0 && (
                    <ResourceBar
                      label="CPU"
                      limit={w.max_cpu ?? w.total_cpu}
                      hardwareTotal={w.total_cpu}
                      reserved={w.allocated_cpu ?? 0}
                      used={w.last_heartbeat.used_cpu_percent}
                      unit="cores"
                      usedIsPercent
                    />
                  )}
                </div>
              ))}
              {!wc.length && (
                <div className="px-4 py-6 text-center text-slate-500 text-sm">No workers connected</div>
              )}
            </div>
          </div>

          <SystemStatusPanel
            workers={{ connected, total: wc.length, draining }}
            summary={summary ? { running, failed } : null}
          />
        </div>
      </div>
    </div>
  );
}
