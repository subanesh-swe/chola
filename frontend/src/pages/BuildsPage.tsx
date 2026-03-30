import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listBuilds } from '../api/builds';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';

const PAGE_SIZE = 20;
const states = ['', 'pending', 'reserved', 'running', 'success', 'failed', 'cancelled'];

export default function BuildsPage() {
  const nav = useNavigate();
  const [page, setPage] = useState(1);
  const [stateFilter, setStateFilter] = useState('');

  const { data, isLoading } = useQuery({
    queryKey: ['builds', page, stateFilter],
    queryFn: () => listBuilds({ limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE, state: stateFilter || undefined }),
    refetchInterval: 5000,
  });

  const builds = data?.job_groups ?? [];
  const total = data?.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">Builds</h2>
        <div className="flex items-center gap-2">
          <label className="text-sm text-slate-400">State:</label>
          <select value={stateFilter} onChange={e => { setStateFilter(e.target.value); setPage(1); }}
            className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white">
            {states.map(s => <option key={s} value={s}>{s || 'All'}</option>)}
          </select>
        </div>
      </div>

      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        {isLoading ? (
          <div className="p-8 text-center text-slate-400">Loading...</div>
        ) : (
          <table className="w-full">
            <thead><tr className="border-b border-slate-700">
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">ID</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Branch</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Commit</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Worker</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
            </tr></thead>
            <tbody className="divide-y divide-slate-800">
              {builds.map(b => (
                <tr key={b.job_group_id} onClick={() => nav(`/builds/${b.job_group_id}`)}
                  className="cursor-pointer hover:bg-slate-800/50 transition-colors">
                  <td className="px-4 py-3"><StatusBadge status={b.state} /></td>
                  <td className="px-4 py-3 text-sm text-slate-300 font-mono">{b.job_group_id.slice(0, 8)}</td>
                  <td className="px-4 py-3 text-sm text-slate-200">{b.branch || '-'}</td>
                  <td className="px-4 py-3 text-sm text-slate-400 font-mono">{b.commit_sha?.slice(0, 7) || '-'}</td>
                  <td className="px-4 py-3 text-sm text-slate-400">{b.reserved_worker_id || '-'}</td>
                  <td className="px-4 py-3 text-sm"><TimeAgo date={b.created_at} className="text-slate-500" /></td>
                </tr>
              ))}
              {!builds.length && <tr><td colSpan={6} className="px-4 py-8 text-center text-slate-500">No builds found</td></tr>}
            </tbody>
          </table>
        )}
      </div>

      {totalPages > 1 && (
        <div className="flex justify-center gap-2">
          <button onClick={() => setPage(p => Math.max(1, p - 1))} disabled={page <= 1}
            className="px-3 py-1 text-sm rounded-lg text-slate-300 hover:bg-slate-800 disabled:text-slate-600">Prev</button>
          <span className="px-3 py-1 text-sm text-slate-400">{page} / {totalPages}</span>
          <button onClick={() => setPage(p => Math.min(totalPages, p + 1))} disabled={page >= totalPages}
            className="px-3 py-1 text-sm rounded-lg text-slate-300 hover:bg-slate-800 disabled:text-slate-600">Next</button>
        </div>
      )}
    </div>
  );
}
