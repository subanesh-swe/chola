import { useQuery } from '@tanstack/react-query';
import { getDashboardSummary } from '../api/dashboard';
import { listBuilds } from '../api/builds';
import { listWorkers } from '../api/workers';
import { StatCard } from '../components/ui/StatCard';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { useNavigate } from 'react-router-dom';

export default function DashboardPage() {
  const nav = useNavigate();
  const { data: summary } = useQuery({ queryKey: ['dashboard'], queryFn: getDashboardSummary, refetchInterval: 5000 });
  const { data: builds } = useQuery({ queryKey: ['builds', 'recent'], queryFn: () => listBuilds({ limit: 10 }), refetchInterval: 5000 });
  const { data: workers } = useQuery({ queryKey: ['workers'], queryFn: listWorkers, refetchInterval: 5000 });

  const wc = workers?.workers ?? [];
  const connected = wc.filter(w => w.status === 'Connected').length;
  const running = summary?.job_groups?.running ?? builds?.job_groups?.filter(b => b.state === 'running').length ?? 0;

  return (
    <div className="space-y-6">
      <h2 className="text-2xl font-bold text-white">Dashboard</h2>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard label="Active Builds" value={running} color="info" />
        <StatCard label="Connected Workers" value={connected} color="success" />
        <StatCard label="Total Workers" value={wc.length} color="default" />
        <StatCard label="Failed Builds" value={summary?.job_groups?.failed ?? 0} color="danger" />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Recent Builds */}
        <div className="bg-slate-900 border border-slate-700 rounded-xl">
          <div className="px-4 py-3 border-b border-slate-700">
            <h3 className="text-sm font-semibold text-slate-300">Recent Builds</h3>
          </div>
          <div className="divide-y divide-slate-800">
            {(builds?.job_groups ?? summary?.recent_builds ?? []).slice(0, 8).map((b: any) => (
              <div key={b.job_group_id} onClick={() => nav(`/builds/${b.job_group_id}`)}
                className="px-4 py-3 flex items-center justify-between hover:bg-slate-800/50 cursor-pointer transition-colors">
                <div className="flex items-center gap-3">
                  <StatusBadge status={b.state} />
                  <div>
                    <p className="text-sm text-slate-200">{b.repo_name || b.branch || b.job_group_id.slice(0, 8)}</p>
                    {b.branch && <p className="text-xs text-slate-500">{b.branch}</p>}
                  </div>
                </div>
                <TimeAgo date={b.created_at} className="text-xs text-slate-500" />
              </div>
            ))}
            {(!builds?.job_groups?.length && !summary?.recent_builds?.length) && (
              <div className="px-4 py-8 text-center text-slate-500">No recent builds</div>
            )}
          </div>
        </div>

        {/* Workers */}
        <div className="bg-slate-900 border border-slate-700 rounded-xl">
          <div className="px-4 py-3 border-b border-slate-700">
            <h3 className="text-sm font-semibold text-slate-300">Workers</h3>
          </div>
          <div className="divide-y divide-slate-800">
            {wc.map(w => (
              <div key={w.worker_id} className="px-4 py-3 flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <StatusBadge status={w.status} />
                  <div>
                    <p className="text-sm text-slate-200">{w.worker_id}</p>
                    <p className="text-xs text-slate-500">{w.hostname}</p>
                  </div>
                </div>
                <div className="text-right text-xs text-slate-400">
                  {w.last_heartbeat && <p>Load: {w.last_heartbeat.system_load.toFixed(1)}</p>}
                  {w.last_heartbeat && <p>Jobs: {w.last_heartbeat.running_jobs}</p>}
                </div>
              </div>
            ))}
            {!wc.length && <div className="px-4 py-8 text-center text-slate-500">No workers connected</div>}
          </div>
        </div>
      </div>
    </div>
  );
}
