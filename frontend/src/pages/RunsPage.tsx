import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listRuns, type Run } from '../api/runs';
import { DataTable, type Column } from '../components/ui/DataTable';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { Pagination } from '../components/ui/Pagination';
import { EmptyState } from '../components/ui/EmptyState';
import { formatSecs } from '../utils/format';

const PAGE_SIZE = 25;
const states = ['', 'queued', 'running', 'success', 'failed', 'cancelled'];

function formatDuration(start: string | null, end: string | null): string {
  if (!start) return '—';
  const s = new Date(start).getTime();
  const e = end ? new Date(end).getTime() : Date.now();
  return formatSecs(Math.round((e - s) / 1000));
}

const columns: Column<Run>[] = [
  {
    key: 'state',
    header: 'Status',
    render: (r) => <StatusBadge status={r.state} />,
  },
  {
    key: 'stage_name',
    header: 'Stage',
    render: (r) => (
      <span className="font-medium text-secondary">{r.stage_name}</span>
    ),
  },
  {
    key: 'repo',
    header: 'Repo',
    render: (r) => (
      <span className="text-muted">{r.repo_name || 'ad-hoc'}</span>
    ),
  },
  {
    key: 'branch',
    header: 'Branch',
    render: (r) => (
      <span className="text-muted">{r.branch || '-'}</span>
    ),
  },
  {
    key: 'worker_id',
    header: 'Worker',
    render: (r) => (
      <span className="text-muted font-mono text-xs">
        {r.worker_id || '-'}
      </span>
    ),
  },
  {
    key: 'duration',
    header: 'Duration',
    render: (r) => (
      <span className="text-muted">
        {formatDuration(r.started_at, r.completed_at)}
      </span>
    ),
  },
  {
    key: 'exit_code',
    header: 'Exit',
    render: (r) => (
      // item 29: render '—' for null exit code
      <span className={r.exit_code === 0 ? 'text-emerald-400' : r.exit_code != null ? 'text-red-400' : 'text-slate-600'}>
        {r.exit_code != null ? r.exit_code : '—'}
      </span>
    ),
  },
  {
    key: 'created_at',
    header: 'Started',
    render: (r) => (
      <TimeAgo date={r.started_at || r.created_at} className="text-disabled" />
    ),
  },
];

export default function RunsPage() {
  const [page, setPage] = useState(1);
  const [stateFilter, setStateFilter] = useState('');
  const [workerFilter, setWorkerFilter] = useState('');
  const [refreshSecs, setRefreshSecs] = useRefreshInterval('runs', 0);

  const { data, isLoading, isError, isFetching, refetch } = useQuery({
    queryKey: ['runs', page, stateFilter, workerFilter],
    queryFn: () =>
      listRuns({
        limit: PAGE_SIZE,
        offset: (page - 1) * PAGE_SIZE,
        state: stateFilter || undefined,
        worker_id: workerFilter || undefined,
      }),
    refetchInterval: refreshSecs > 0 ? refreshSecs * 1000 : false,
    placeholderData: keepPreviousData,
  });

  const runs = data?.data ?? [];
  const total = data?.pagination.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-primary">Runs</h2>
        <div className="flex items-center gap-3 flex-wrap">
          <div className="flex items-center gap-2">
            <label htmlFor="run-state" className="text-sm text-muted">
              State:
            </label>
            <select
              id="run-state"
              value={stateFilter}
              onChange={(e) => {
                setStateFilter(e.target.value);
                setPage(1);
              }}
              className="bg-surface-2 border border-border rounded-lg px-3 py-1.5 text-sm text-primary focus:outline-none focus:ring-2 focus:ring-accent"
            >
              {states.map((s) => (
                <option key={s} value={s}>
                  {s || 'All'}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center gap-2">
            <label htmlFor="run-worker" className="text-sm text-muted">
              Worker:
            </label>
            <input
              id="run-worker"
              type="text"
              placeholder="worker-id"
              value={workerFilter}
              onChange={(e) => {
                setWorkerFilter(e.target.value);
                setPage(1);
              }}
              className="bg-surface-2 border border-border rounded-lg px-3 py-1.5 text-sm text-primary w-36 focus:outline-none focus:ring-2 focus:ring-accent"
            />
          </div>
          <RefreshControl
            intervalSecs={refreshSecs}
            onIntervalChange={setRefreshSecs}
            onRefresh={() => refetch()}
            isFetching={isFetching}
          />
        </div>
      </div>

      {isError && (
        <div
          role="alert"
          className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400"
        >
          Failed to load runs. Please try again.
        </div>
      )}

      {/* item 27: use EmptyState when no runs */}
      {!isLoading && runs.length === 0 ? (
        <EmptyState title="No runs found" description="Runs will appear here once stages execute." />
      ) : (
        <DataTable
          data={runs}
          columns={columns}
          keyExtractor={(r) => r.id}
          rowHref={(r) => `/builds/${r.job_group_id}`}
          rowAriaLabel={(r) => `${r.stage_name} on ${r.repo_name ?? 'ad-hoc'} — ${r.state}`}
          emptyMessage="No runs found"
          loading={isLoading}
        />
      )}

      <Pagination page={page} totalPages={totalPages} onPageChange={setPage} />
    </div>
  );
}
