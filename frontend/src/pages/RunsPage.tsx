import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listRuns, type Run } from '../api/runs';
import { DataTable, type Column } from '../components/ui/DataTable';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { Pagination } from '../components/ui/Pagination';

const PAGE_SIZE = 25;
const states = ['', 'queued', 'running', 'success', 'failed', 'cancelled'];

function formatDuration(start: string | null, end: string | null): string {
  if (!start) return '-';
  const s = new Date(start).getTime();
  const e = end ? new Date(end).getTime() : Date.now();
  const secs = Math.round((e - s) / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  const rem = secs % 60;
  return `${mins}m ${rem}s`;
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
      <span className="font-medium text-slate-200">{r.stage_name}</span>
    ),
  },
  {
    key: 'repo',
    header: 'Repo',
    render: (r) => (
      <span className="text-slate-400">{r.repo_name || 'ad-hoc'}</span>
    ),
  },
  {
    key: 'branch',
    header: 'Branch',
    render: (r) => (
      <span className="text-slate-400">{r.branch || '-'}</span>
    ),
  },
  {
    key: 'worker_id',
    header: 'Worker',
    render: (r) => (
      <span className="text-slate-400 font-mono text-xs">
        {r.worker_id || '-'}
      </span>
    ),
  },
  {
    key: 'duration',
    header: 'Duration',
    render: (r) => (
      <span className="text-slate-400">
        {formatDuration(r.started_at, r.completed_at)}
      </span>
    ),
  },
  {
    key: 'exit_code',
    header: 'Exit',
    render: (r) => (
      <span className={r.exit_code === 0 ? 'text-emerald-400' : r.exit_code != null ? 'text-red-400' : 'text-slate-500'}>
        {r.exit_code != null ? r.exit_code : '-'}
      </span>
    ),
  },
  {
    key: 'created_at',
    header: 'Started',
    render: (r) => (
      <TimeAgo date={r.started_at || r.created_at} className="text-slate-500" />
    ),
  },
];

export default function RunsPage() {
  const nav = useNavigate();
  const [page, setPage] = useState(1);
  const [stateFilter, setStateFilter] = useState('');
  const [workerFilter, setWorkerFilter] = useState('');

  const { data, isLoading, isError } = useQuery({
    queryKey: ['runs', page, stateFilter, workerFilter],
    queryFn: () =>
      listRuns({
        limit: PAGE_SIZE,
        offset: (page - 1) * PAGE_SIZE,
        state: stateFilter || undefined,
        worker_id: workerFilter || undefined,
      }),
    refetchInterval: 5000,
  });

  const runs = data?.data ?? [];
  const total = data?.pagination.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-white">Runs</h2>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <label htmlFor="run-state" className="text-sm text-slate-400">
              State:
            </label>
            <select
              id="run-state"
              value={stateFilter}
              onChange={(e) => {
                setStateFilter(e.target.value);
                setPage(1);
              }}
              className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              {states.map((s) => (
                <option key={s} value={s}>
                  {s || 'All'}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center gap-2">
            <label htmlFor="run-worker" className="text-sm text-slate-400">
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
              className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white w-36 focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>
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

      <DataTable
        data={runs}
        columns={columns}
        keyExtractor={(r) => r.id}
        onRowClick={(r) => nav(`/builds/${r.job_group_id}`)}
        emptyMessage="No runs found"
        loading={isLoading}
      />

      <Pagination page={page} totalPages={totalPages} onPageChange={setPage} />
    </div>
  );
}
