import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { listBuildsRaw } from '../api/builds';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';

export default function BuildQueuePage() {
  const pendingQ = useQuery({
    queryKey: ['queue', 'pending'],
    queryFn: () => listBuildsRaw({ limit: 100, state: 'pending' }),
    refetchInterval: 5000,
  });

  const reservedQ = useQuery({
    queryKey: ['queue', 'reserved'],
    queryFn: () => listBuildsRaw({ limit: 100, state: 'reserved' }),
    refetchInterval: 5000,
  });

  const isLoading = pendingQ.isLoading || reservedQ.isLoading;
  const isError = pendingQ.isError || reservedQ.isError;
  const pendingCount = pendingQ.data?.pagination.total ?? 0;
  const reservedCount = reservedQ.data?.pagination.total ?? 0;

  const queueItems = [
    ...(pendingQ.data?.data ?? []),
    ...(reservedQ.data?.data ?? []),
  ].sort((a, b) => a.created_at.localeCompare(b.created_at));

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 className="text-2xl font-bold text-white">Build Queue</h2>
          <p className="text-sm text-slate-400 mt-0.5">Jobs waiting to run</p>
        </div>
        <div className="flex items-center gap-4 text-sm">
          <span className="text-slate-400">
            <span className="text-white font-semibold">{pendingCount}</span> pending
          </span>
          <span className="text-slate-400">
            <span className="text-white font-semibold">{reservedCount}</span> reserved
          </span>
        </div>
      </div>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load build queue. Please try again.
        </div>
      )}
      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        {isLoading ? (
          <div className="p-8 text-center text-slate-400">Loading…</div>
        ) : (
          <>
            {/* Desktop table */}
            <div className="hidden sm:block overflow-x-auto">
              <table className="w-full" aria-label="Build queue">
                <thead>
                  <tr className="border-b border-slate-700">
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase w-12">#</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">ID</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Repo</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Branch</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Worker</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Submitted</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800">
                  {queueItems.map((job, idx) => {
                    const href = `/builds/${job.job_group_id}`;
                    return (
                      <tr
                        key={job.job_group_id}
                        className="hover:bg-slate-800/50 transition-colors"
                      >
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-500 tabular-nums focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {idx + 1}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            <StatusBadge status={job.state} />
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-300 font-mono focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {job.job_group_id.slice(0, 8)}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-300 max-w-[180px] truncate focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {job.repo_name ?? job.repo_id?.slice(0, 8) ?? '-'}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {job.branch ?? '-'}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm text-slate-400 font-mono focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            {job.reserved_worker_id ? job.reserved_worker_id.slice(0, 8) : '-'}
                          </Link>
                        </td>
                        <td className="p-0">
                          <Link to={href} className="block px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                            <TimeAgo date={job.created_at} className="text-slate-500" />
                          </Link>
                        </td>
                      </tr>
                    );
                  })}
                  {!queueItems.length && (
                    <tr>
                      <td colSpan={7} className="px-4 py-12 text-center text-slate-500">
                        Queue is empty
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>

            {/* Mobile stacked cards */}
            <div className="sm:hidden divide-y divide-slate-800">
              {queueItems.map((job, idx) => (
                <Link
                  key={job.job_group_id}
                  to={`/builds/${job.job_group_id}`}
                  aria-label={`Queue position ${idx + 1}: ${job.branch ?? 'unknown branch'}`}
                  className="block w-full px-4 py-3 hover:bg-slate-800/50 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-inset"
                >
                  <div className="flex items-center justify-between mb-1">
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-slate-600 tabular-nums">#{idx + 1}</span>
                      <StatusBadge status={job.state} />
                    </div>
                    <TimeAgo date={job.created_at} className="text-xs text-slate-500" />
                  </div>
                  <div className="text-sm text-slate-200">{job.branch ?? '-'}</div>
                  <div className="text-xs text-slate-500 mt-0.5">
                    {job.repo_name ?? job.repo_id?.slice(0, 8) ?? '-'}
                    {job.reserved_worker_id && (
                      <span className="ml-2 font-mono">{job.reserved_worker_id.slice(0, 8)}</span>
                    )}
                  </div>
                </Link>
              ))}
              {!queueItems.length && (
                <div className="px-4 py-12 text-center text-slate-500">Queue is empty</div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
