import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { listBuilds } from '../api/builds';
import { listRepos } from '../api/repos';
import { useUrlFilters } from '../hooks/useUrlFilters';
import { FilterBar } from '../components/ui/FilterBar';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { TableSkeleton } from '../components/ui/PageSkeleton';

export default function BuildsPage() {
  const { filters, setFilters, resetFilters } = useUrlFilters();

  const { data: reposData } = useQuery({
    queryKey: ['repos'],
    queryFn: () => listRepos({ limit: 100 }),
  });
  const repos = reposData?.data ?? [];

  const { data, isLoading, isError } = useQuery({
    queryKey: ['builds', filters],
    queryFn: () => listBuilds(filters),
    refetchInterval: 5000,
  });

  const builds = data?.data ?? [];
  const total = data?.pagination.total ?? 0;
  const PAGE_SIZE = 20;
  const totalPages = Math.ceil(total / PAGE_SIZE);
  const page = filters.page;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-white">Builds</h2>
      </div>

      <FilterBar filters={filters} repos={repos} onChange={setFilters} onReset={resetFilters} />

      {/* Show archived toggle */}
      <label className="inline-flex items-center gap-2 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={filters.includeArchived}
          onChange={(e) => setFilters({ includeArchived: e.target.checked, page: 1 })}
          className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-blue-500 focus:ring-blue-500 focus:ring-offset-slate-900"
        />
        <span className="text-sm text-slate-400">Show archived builds</span>
      </label>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load builds. Please try again.
        </div>
      )}
      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        {/* item 11: table skeleton instead of Loading text */}
        {isLoading ? (
          <TableSkeleton rows={6} cols={6} />
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
                  {builds.map(b => {
                    const href = `/builds/${b.job_group_id}`;
                    return (
                      <tr key={b.job_group_id} className={`hover:bg-slate-800/50 transition-colors${b.archived ? ' opacity-60' : ''}`}>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            <div className="flex items-center gap-2 flex-wrap">
                              <StatusBadge status={b.state} />
                              {b.archived && <StatusBadge status="archived" />}
                            </div>
                            {b.status_reason && (
                              <span className="block text-[10px] text-slate-500 truncate max-w-[180px]">{b.status_reason}</span>
                            )}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-300 font-mono focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {b.job_group_id.slice(0, 8)}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {b.branch || '-'}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-400 font-mono focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {b.commit_sha?.slice(0, 7) || '-'}
                          </Link>
                        </td>
                        {/* item 12: ?? instead of || to avoid showing "undefined" */}
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-400 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {b.reserved_worker_id ?? '-'}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            <TimeAgo date={b.created_at} className="text-slate-500" />
                          </Link>
                        </td>
                      </tr>
                    );
                  })}
                  {!builds.length && (
                    <tr><td colSpan={6} className="px-4 py-8 text-center text-slate-500">No builds found</td></tr>
                  )}
                </tbody>
              </table>
            </div>

            {/* Mobile stacked cards */}
            <div className="sm:hidden divide-y divide-slate-800">
              {builds.map(b => (
                <Link
                  key={b.job_group_id}
                  to={`/builds/${b.job_group_id}`}
                  className={`block w-full px-4 py-3 hover:bg-slate-800/50 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-inset${b.archived ? ' opacity-60' : ''}`}
                >
                  <div className="flex items-center justify-between mb-1">
                    <div className="flex items-center gap-2">
                      <StatusBadge status={b.state} />
                      {b.archived && <StatusBadge status="archived" />}
                    </div>
                    <TimeAgo date={b.created_at} className="text-xs text-slate-500" />
                  </div>
                  {b.status_reason && (
                    <span className="block text-[10px] text-slate-500 truncate max-w-xs">{b.status_reason}</span>
                  )}
                  <div className="text-sm text-slate-300 font-mono">{b.job_group_id.slice(0, 8)}</div>
                  <div className="text-sm text-slate-400 mt-0.5">
                    {b.branch || '-'}
                    {b.commit_sha && <span className="ml-2 font-mono text-slate-500">{b.commit_sha.slice(0, 7)}</span>}
                  </div>
                </Link>
              ))}
              {!builds.length && (
                <div className="px-4 py-8 text-center text-slate-500">No builds found</div>
              )}
            </div>
          </>
        )}
      </div>

      {/* item 14: gap-3 between pagination buttons and indicator */}
      {totalPages > 1 && (
        <div className="flex justify-center gap-3">
          <button
            onClick={() => setFilters({ page: Math.max(1, page - 1) })}
            disabled={page <= 1}
            className="px-3 py-1 text-sm rounded-lg text-slate-300 hover:bg-slate-800 disabled:text-slate-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Prev
          </button>
          <span className="px-3 py-1 text-sm text-slate-400">{page} / {totalPages}</span>
          <button
            onClick={() => setFilters({ page: Math.min(totalPages, page + 1) })}
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
