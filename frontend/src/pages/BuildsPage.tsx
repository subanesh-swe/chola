import { useState, useEffect } from 'react';
import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { listBuilds } from '../api/builds';
import { useAppliedFilters } from '../hooks/useAppliedFilters';
import { useRefreshInterval } from '../hooks/useRefreshInterval';
import { useQueryHistory } from '../hooks/useQueryHistory';
import { FilterBar } from '../components/ui/FilterBar';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { TableSkeleton } from '../components/ui/PageSkeleton';

export default function BuildsPage() {
  const { applied, draft, patchDraft, apply, applyPatch, reset, isDirty } = useAppliedFilters();
  const [refreshSecs, setRefreshSecs] = useRefreshInterval('builds', 5);
  // queryValue mirrors applied.q so the QueryBox reflects the active ChQL query.
  const [queryValue, setQueryValue] = useState(applied.q);
  const historyApi = useQueryHistory('builds');

  // Seed date defaults on first load when URL has no date filter.
  useEffect(() => {
    if (!applied.dateFrom && !applied.dateTo) {
      applyPatch({ dateFrom: 'now-24h', dateTo: 'now' });
    }
    // Only on mount — intentionally omitting deps.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const { data, isLoading, isError, isFetching, refetch } = useQuery({
    queryKey: ['builds', applied],
    queryFn: () => listBuilds(applied),
    refetchInterval: refreshSecs > 0 ? refreshSecs * 1000 : false,
    placeholderData: keepPreviousData,
  });

  const builds = data?.data ?? [];
  const total = data?.pagination.total ?? 0;
  const PAGE_SIZE = 20;
  const totalPages = Math.ceil(total / PAGE_SIZE);
  const page = applied.page;

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-bold text-primary">Builds</h2>

      <FilterBar
        filters={draft}
        queryValue={queryValue}
        onQueryChange={setQueryValue}
        onChange={patchDraft}
        onApply={apply}
        onReset={reset}
        isDirty={isDirty}
        isFetching={isFetching}
        onPresetApply={applyPatch}
        historyApi={historyApi}
        refreshSecs={refreshSecs}
        onIntervalChange={setRefreshSecs}
        onRefresh={() => refetch()}
      />

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
      <div className="bg-surface border border-border rounded-xl overflow-hidden">
        {isLoading ? (
          <TableSkeleton rows={6} cols={6} />
        ) : (
          <>
            {/* Desktop table */}
            <div className="hidden sm:block overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-border">
                    <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Status</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">ID</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Branch</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Commit</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Worker</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Created</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border">
                  {builds.map(b => {
                    const href = `/builds/${b.job_group_id}`;
                    return (
                      <tr key={b.job_group_id} className={`relative hover:bg-surface-hover/50 transition-colors${b.archived ? ' opacity-60' : ''}`}>
                        <td className="px-4 py-3">
                          <Link to={href} aria-label={`Build ${b.job_group_id.slice(0, 8)}${b.branch ? ` on ${b.branch}` : ''} — ${b.state}${b.archived ? ' (archived)' : ''}`} className="absolute inset-0 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-accent" />
                          <span className="relative z-10">
                            <div className="flex items-center gap-2 flex-wrap">
                              <StatusBadge status={b.state} />
                              {b.archived && <StatusBadge status="archived" />}
                            </div>
                            {b.status_reason && (
                              <span className="block text-[10px] text-disabled truncate max-w-[180px]">{b.status_reason}</span>
                            )}
                          </span>
                        </td>
                        <td className="px-4 py-3 text-sm text-secondary font-mono relative z-10">{b.job_group_id.slice(0, 8)}</td>
                        <td className="px-4 py-3 text-sm text-secondary relative z-10">{b.branch || '-'}</td>
                        <td className="px-4 py-3 text-sm text-muted font-mono relative z-10">{b.commit_sha?.slice(0, 7) || '-'}</td>
                        <td className="px-4 py-3 text-sm text-muted relative z-10">{b.reserved_worker_id ?? '-'}</td>
                        <td className="px-4 py-3 text-sm relative z-10">
                          <TimeAgo date={b.created_at} className="text-disabled" />
                        </td>
                      </tr>
                    );
                  })}
                  {!builds.length && (
                    <tr><td colSpan={6} className="px-4 py-8 text-center text-disabled">No builds found</td></tr>
                  )}
                </tbody>
              </table>
            </div>

            {/* Mobile stacked cards */}
            <div className="sm:hidden divide-y divide-border">
              {builds.map(b => (
                <Link
                  key={b.job_group_id}
                  to={`/builds/${b.job_group_id}`}
                  className={`block w-full px-4 py-3 hover:bg-surface-hover/50 transition-colors focus:outline-none focus:ring-2 focus:ring-accent focus:ring-inset${b.archived ? ' opacity-60' : ''}`}
                >
                  <div className="flex items-center justify-between mb-1">
                    <div className="flex items-center gap-2">
                      <StatusBadge status={b.state} />
                      {b.archived && <StatusBadge status="archived" />}
                    </div>
                    <TimeAgo date={b.created_at} className="text-xs text-disabled" />
                  </div>
                  {b.status_reason && (
                    <span className="block text-[10px] text-disabled truncate max-w-xs">{b.status_reason}</span>
                  )}
                  <div className="text-sm text-secondary font-mono">{b.job_group_id.slice(0, 8)}</div>
                  <div className="text-sm text-muted mt-0.5">
                    {b.branch || '-'}
                    {b.commit_sha && <span className="ml-2 font-mono text-disabled">{b.commit_sha.slice(0, 7)}</span>}
                  </div>
                </Link>
              ))}
              {!builds.length && (
                <div className="px-4 py-8 text-center text-disabled">No builds found</div>
              )}
            </div>
          </>
        )}
      </div>

      {totalPages > 1 && (
        <div className="flex justify-center gap-3">
          <button
            onClick={() => applyPatch({ page: Math.max(1, page - 1) })}
            disabled={page <= 1}
            className="px-3 py-1 text-sm rounded-lg text-secondary hover:bg-surface-hover disabled:text-disabled disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Prev
          </button>
          <span className="px-3 py-1 text-sm text-muted">{page} / {totalPages}</span>
          <button
            onClick={() => applyPatch({ page: Math.min(totalPages, page + 1) })}
            disabled={page >= totalPages}
            className="px-3 py-1 text-sm rounded-lg text-secondary hover:bg-surface-hover disabled:text-disabled disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
