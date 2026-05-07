import { useState, type ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { clsx } from 'clsx';

export interface Column<T> {
  key: string;
  header: string;
  render: (row: T) => ReactNode;
  sortable?: boolean;
  className?: string;
}

interface Props<T> {
  data: T[];
  columns: Column<T>[];
  keyExtractor: (row: T) => string;
  onRowClick?: (row: T) => void;
  /** When set, each row becomes a real anchor (supports middle-click / Cmd+click). */
  rowHref?: (row: T) => string;
  /** Provides a meaningful aria-label for the row link (first cell). Falls back to keyExtractor. */
  rowAriaLabel?: (row: T) => string;
  emptyMessage?: string;
  loading?: boolean;
}

export function DataTable<T>({
  data,
  columns,
  keyExtractor,
  onRowClick,
  rowHref,
  rowAriaLabel,
  emptyMessage = 'No data',
  loading = false,
}: Props<T>) {
  const [sortKey, setSortKey] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc');

  const handleSort = (key: string) => {
    if (sortKey === key) {
      setSortDir(sortDir === 'asc' ? 'desc' : 'asc');
    } else {
      setSortKey(key);
      setSortDir('asc');
    }
  };

  const isClickable = !!(onRowClick || rowHref);

  if (loading) {
    return (
      <div className="bg-surface border border-border rounded-xl overflow-hidden">
        <div className="p-8 text-center text-muted">Loading...</div>
      </div>
    );
  }

  return (
    <div className="bg-surface border border-border rounded-xl overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full">
          <thead>
            <tr className="border-b border-border">
              {columns.map((col) => (
                <th
                  key={col.key}
                  scope="col"
                  aria-sort={
                    col.sortable
                      ? sortKey === col.key
                        ? sortDir === 'asc'
                          ? 'ascending'
                          : 'descending'
                        : 'none'
                      : undefined
                  }
                  className={clsx(
                    'px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider',
                    col.sortable && 'cursor-pointer hover:text-secondary',
                    col.className,
                  )}
                  onClick={col.sortable ? () => handleSort(col.key) : undefined}
                  onKeyDown={
                    col.sortable
                      ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleSort(col.key); } }
                      : undefined
                  }
                  tabIndex={col.sortable ? 0 : undefined}
                >
                  {col.header}
                  {sortKey === col.key && (
                    <span className="ml-1" aria-hidden="true">{sortDir === 'asc' ? '↑' : '↓'}</span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {data.length === 0 ? (
              <tr>
                <td colSpan={columns.length} className="px-4 py-8 text-center text-disabled">
                  {emptyMessage}
                </td>
              </tr>
            ) : (
              data.map((row) => {
                const href = rowHref ? rowHref(row) : undefined;
                const rowKey = keyExtractor(row);
                return (
                  <tr
                    key={rowKey}
                    onClick={onRowClick && !rowHref ? () => onRowClick(row) : undefined}
                    onKeyDown={
                      onRowClick && !rowHref
                        ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onRowClick(row); } }
                        : undefined
                    }
                    tabIndex={onRowClick && !rowHref ? 0 : undefined}
                    className={clsx(
                      'relative transition-colors',
                      isClickable && 'hover:bg-slate-800/50',
                      onRowClick && !rowHref && 'cursor-pointer focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500',
                    )}
                  >
                    {columns.map((col, colIdx) => (
                      <td
                        key={col.key}
                        className={clsx(
                          'px-4 py-3 text-sm text-slate-200',
                          col.className,
                        )}
                      >
                        {/* Stretched invisible anchor covers the whole row — first cell only */}
                        {href && colIdx === 0 && (
                          <Link
                            to={href}
                            aria-label={rowKey}
                            tabIndex={0}
                            className="absolute inset-0 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500"
                          />
                        )}
                        {/* Interactive children (e.g. buttons) sit above the stretched link */}
                        <span className={clsx(href && 'relative z-10')}>
                          {col.render(row)}
                        </span>
                      </td>
                    ))}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
