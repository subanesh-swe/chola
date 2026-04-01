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

  const { data, isLoading, isError } = useQuery({
    queryKey: ['builds', page, stateFilter],
    queryFn: () => listBuilds({ limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE, state: stateFilter || undefined }),
    refetchInterval: 5000,
  });

  const builds = data?.data ?? [];
  const total = data?.pagination.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-white">Builds</h2>
        <div className="flex items-center gap-2">
          <label htmlFor="state-filter" className="text-sm text-slate-400">State:</label>
          <select
            id="state-filter"
            value={stateFilter}
            onChange={e => { setStateFilter(e.target.value); setPage(1); }}
            className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            {states.map(s => <option key={s} value={s}>{s || 'All'}</option>)}
          </select>
        </div>
      </div>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load builds. Please try again.
        </div>
      )}
      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        {isLoading ? (
          <div className="p-8 text-center text-slate-400">Loading...</div>
        ) : (
          <>
            {/* Desktop table */}
            <div className="hidden sm:block overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-slate-700">
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">ID</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Branch</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Commit</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Worker</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800">
                  {builds.map(b => (
                    <tr
                      key={b.job_group_id}
                      onClick={() => nav(`/builds/${b.job_group_id}`)}
                      onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') nav(`/builds/${b.job_group_id}`); }}
                      tabIndex={0}
                      role="row"
                      className="cursor-pointer hover:bg-slate-800/50 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-inset"
                    >
                      <td className="px-4 py-3"><StatusBadge status={b.state} /></td>
                      <td className="px-4 py-3 text-sm text-slate-300 font-mono">{b.job_group_id.slice(0, 8)}</td>
                      <td className="px-4 py-3 text-sm text-slate-200">{b.branch || '-'}</td>
                      <td className="px-4 py-3 text-sm text-slate-400 font-mono">{b.commit_sha?.slice(0, 7) || '-'}</td>
                      <td className="px-4 py-3 text-sm text-slate-400">{b.reserved_worker_id || '-'}</td>
                      <td className="px-4 py-3 text-sm"><TimeAgo date={b.created_at} className="text-slate-500" /></td>
                    </tr>
                  ))}
                  {!builds.length && (
                    <tr><td colSpan={6} className="px-4 py-8 text-center text-slate-500">No builds found</td></tr>
                  )}
                </tbody>
              </table>
            </div>

            {/* Mobile stacked cards */}
            <div className="sm:hidden divide-y divide-slate-800">
              {builds.map(b => (
                <button
                  key={b.job_group_id}
                  onClick={() => nav(`/builds/${b.job_group_id}`)}
                  className="w-full text-left px-4 py-3 hover:bg-slate-800/50 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-inset"
                >
                  <div className="flex items-center justify-between mb-1">
                    <StatusBadge status={b.state} />
                    <TimeAgo date={b.created_at} className="text-xs text-slate-500" />
                  </div>
                  <div className="text-sm text-slate-300 font-mono">{b.job_group_id.slice(0, 8)}</div>
                  <div className="text-sm text-slate-400 mt-0.5">
                    {b.branch || '-'}
                    {b.commit_sha && <span className="ml-2 font-mono text-slate-500">{b.commit_sha.slice(0, 7)}</span>}
                  </div>
                </button>
              ))}
              {!builds.length && (
                <div className="px-4 py-8 text-center text-slate-500">No builds found</div>
              )}
            </div>
          </>
        )}
      </div>

      {totalPages > 1 && (
        <div className="flex justify-center gap-2">
          <button
            onClick={() => setPage(p => Math.max(1, p - 1))}
            disabled={page <= 1}
            className="px-3 py-1 text-sm rounded-lg text-slate-300 hover:bg-slate-800 disabled:text-slate-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Prev
          </button>
          <span className="px-3 py-1 text-sm text-slate-400">{page} / {totalPages}</span>
          <button
            onClick={() => setPage(p => Math.min(totalPages, p + 1))}
            disabled={page >= totalPages}
            className="px-3 py-1 text-sm rounded-lg text-slate-300 hover:bg-slate-800 disabled:text-slate-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
