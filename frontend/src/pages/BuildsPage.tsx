import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listBuilds } from '../api/builds';
import { listRepos } from '../api/repos';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { DataTable, type Column } from '../components/ui/DataTable';
import { Pagination } from '../components/ui/Pagination';
import { FilterBar } from '../components/ui/FilterBar';
import { useUrlFilters } from '../hooks/useUrlFilters';
import type { JobGroup } from '../types';

const PAGE_SIZE = 20;

function buildQueryParams(filters: ReturnType<typeof useUrlFilters>['filters']) {
  return {
    limit: PAGE_SIZE,
    offset: (filters.page - 1) * PAGE_SIZE,
    state: filters.state.length ? filters.state.join(',') : undefined,
    repo_id: filters.repo || undefined,
    branch: filters.branch || undefined,
    date_from: filters.dateFrom || undefined,
    date_to: filters.dateTo || undefined,
    sort_by: filters.sortKey || undefined,
    sort_dir: filters.sortKey ? filters.sortDir : undefined,
  };
}

function useBuildsColumns(nav: ReturnType<typeof useNavigate>): Column<JobGroup>[] {
  return [
    { key: 'state', header: 'Status', render: (b) => <StatusBadge status={b.state} /> },
    { key: 'job_group_id', header: 'ID', render: (b) => <span className="font-mono text-slate-300">{b.job_group_id.slice(0, 8)}</span> },
    { key: 'branch', header: 'Branch', sortable: true, render: (b) => b.branch || '-' },
    { key: 'commit_sha', header: 'Commit', render: (b) => <span className="font-mono text-slate-400">{b.commit_sha?.slice(0, 7) ?? '-'}</span> },
    { key: 'reserved_worker_id', header: 'Worker', render: (b) => <span className="text-slate-400">{b.reserved_worker_id || '-'}</span> },
    { key: 'created_at', header: 'Created', sortable: true, render: (b) => <TimeAgo date={b.created_at} className="text-slate-500" /> },
  ];
}

export default function BuildsPage() {
  const nav = useNavigate();
  const { filters, setFilters, resetFilters } = useUrlFilters();
  const columns = useBuildsColumns(nav);

  const { data, isLoading } = useQuery({
    queryKey: ['builds', filters],
    queryFn: () => listBuilds(buildQueryParams(filters)),
    refetchInterval: 5000,
  });

  const { data: reposData } = useQuery({
    queryKey: ['repos'],
    queryFn: listRepos,
  });

  const builds = data?.job_groups ?? [];
  const totalPages = Math.ceil((data?.total ?? 0) / PAGE_SIZE);

  const handleSort = (key: string) => {
    if (filters.sortKey === key) {
      setFilters({ sortDir: filters.sortDir === 'asc' ? 'desc' : 'asc', page: 1 });
    } else {
      setFilters({ sortKey: key, sortDir: 'asc', page: 1 });
    }
  };

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-bold text-white">Builds</h2>

      <FilterBar
        filters={filters}
        repos={reposData?.repos ?? []}
        onChange={setFilters}
        onReset={resetFilters}
      />

      <DataTable
        data={builds}
        columns={columns}
        keyExtractor={(b) => b.job_group_id}
        onRowClick={(b) => nav(`/builds/${b.job_group_id}`)}
        onSort={handleSort}
        sortKey={filters.sortKey}
        sortDir={filters.sortDir}
        loading={isLoading}
        emptyMessage="No builds found"
      />

      <Pagination page={filters.page} totalPages={totalPages} onPageChange={(p) => setFilters({ page: p })} />
    </div>
  );
}
