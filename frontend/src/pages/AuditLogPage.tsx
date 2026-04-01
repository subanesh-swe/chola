import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listAuditLogs } from '../api/audit';
import type { AuditEntry } from '../api/audit';
import { DataTable } from '../components/ui/DataTable';
import type { Column } from '../components/ui/DataTable';
import { TimeAgo } from '../components/ui/TimeAgo';

const PAGE_SIZE = 50;

export default function AuditLogPage() {
  const [filterUser, setFilterUser] = useState('');
  const [filterAction, setFilterAction] = useState('');
  const [filterDate, setFilterDate] = useState('');
  const [page, setPage] = useState(0);

  const { data, isLoading, isError } = useQuery({
    queryKey: ['audit-log'],
    queryFn: () => listAuditLogs(500),
  });

  const entries = data?.entries ?? [];

  const filtered = useMemo(() => {
    return entries.filter((e) => {
      if (filterUser && !e.username.toLowerCase().includes(filterUser.toLowerCase())) return false;
      if (filterAction && !e.action.toLowerCase().includes(filterAction.toLowerCase())) return false;
      if (filterDate && !e.created_at.startsWith(filterDate)) return false;
      return true;
    });
  }, [entries, filterUser, filterAction, filterDate]);

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const pageData = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);

  const columns: Column<AuditEntry>[] = [
    {
      key: 'created_at',
      header: 'Timestamp',
      sortable: true,
      render: (e) => <TimeAgo date={e.created_at} className="text-slate-400 text-xs" />,
    },
    {
      key: 'username',
      header: 'User',
      sortable: true,
      render: (e) => <span className="text-slate-200">{e.username}</span>,
    },
    {
      key: 'action',
      header: 'Action',
      sortable: true,
      render: (e) => (
        <span className="text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300 font-mono">
          {e.action}
        </span>
      ),
    },
    {
      key: 'resource_type',
      header: 'Resource',
      render: (e) =>
        e.resource_type ? (
          <span className="text-slate-300">
            {e.resource_type}
            {e.resource_id && (
              <span className="text-slate-500 text-xs ml-1 font-mono">
                {e.resource_id.length > 12 ? e.resource_id.slice(0, 8) + '…' : e.resource_id}
              </span>
            )}
          </span>
        ) : (
          <span className="text-slate-600">—</span>
        ),
    },
    {
      key: 'ip_address',
      header: 'IP',
      render: (e) => (
        <span className="text-slate-500 text-xs font-mono">{e.ip_address ?? '—'}</span>
      ),
    },
  ];

  if (isLoading) return <div className="text-slate-400">Loading...</div>;
  if (isError) return (
    <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
      Failed to load audit log. Please try again.
    </div>
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-white">
          Audit Log
          <span className="ml-2 text-sm font-normal text-slate-500">({filtered.length} events)</span>
        </h2>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-3">
        <input
          type="text"
          placeholder="Filter by user..."
          value={filterUser}
          onChange={(e) => { setFilterUser(e.target.value); setPage(0); }}
          aria-label="Filter by username"
          className="px-3 py-1.5 text-sm bg-slate-800 border border-slate-700 rounded-lg text-slate-200 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <input
          type="text"
          placeholder="Filter by action..."
          value={filterAction}
          onChange={(e) => { setFilterAction(e.target.value); setPage(0); }}
          aria-label="Filter by action"
          className="px-3 py-1.5 text-sm bg-slate-800 border border-slate-700 rounded-lg text-slate-200 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <input
          type="date"
          value={filterDate}
          onChange={(e) => { setFilterDate(e.target.value); setPage(0); }}
          aria-label="Filter by date"
          className="px-3 py-1.5 text-sm bg-slate-800 border border-slate-700 rounded-lg text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        {(filterUser || filterAction || filterDate) && (
          <button
            onClick={() => { setFilterUser(''); setFilterAction(''); setFilterDate(''); setPage(0); }}
            className="px-3 py-1.5 text-sm text-slate-400 hover:text-white bg-slate-800 border border-slate-700 rounded-lg"
          >
            Clear
          </button>
        )}
      </div>

      <DataTable
        data={pageData}
        columns={columns}
        keyExtractor={(e) => e.id}
        emptyMessage="No audit events found"
        loading={isLoading}
      />

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between text-sm text-slate-400">
          <span>
            Page {page + 1} of {totalPages}
          </span>
          <div className="flex gap-2">
            <button
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              disabled={page === 0}
              className="px-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg disabled:opacity-40 hover:bg-slate-700"
            >
              Prev
            </button>
            <button
              onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
              disabled={page >= totalPages - 1}
              className="px-3 py-1.5 bg-slate-800 border border-slate-700 rounded-lg disabled:opacity-40 hover:bg-slate-700"
            >
              Next
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
