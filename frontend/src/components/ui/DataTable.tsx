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
      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        <div className="p-8 text-center text-slate-400">Loading...</div>
      </div>
    );
  }

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full">
          <thead>
            <tr className="border-b border-slate-700">
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
                    'px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase tracking-wider',
                    col.sortable && 'cursor-pointer hover:text-slate-200',
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
          <tbody className="divide-y divide-slate-800">
            {data.length === 0 ? (
              <tr>
                <td colSpan={columns.length} className="px-4 py-8 text-center text-slate-500">
                  {emptyMessage}
                </td>
              </tr>
            ) : (
              data.map((row) => {
                const href = rowHref ? rowHref(row) : undefined;
                return (
                  <tr
                    key={keyExtractor(row)}
                    onClick={onRowClick && !rowHref ? () => onRowClick(row) : undefined}
                    onKeyDown={
                      onRowClick && !rowHref
                        ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onRowClick(row); } }
                        : undefined
                    }
                    tabIndex={onRowClick && !rowHref ? 0 : undefined}
                    className={clsx(
                      'transition-colors',
                      // item 46: active bg for touch feedback
                      isClickable && 'hover:bg-slate-800/50 active:bg-slate-800',
                      onRowClick && !rowHref && 'cursor-pointer focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500',
                    )}
                  >
                    {/* Stretched link relies on row-level position:relative; supported in Safari 16+, Chromium 88+, Firefox 90+. */}
                    {columns.map((col, colIdx) => (
                      <td
                        key={col.key}
                        className={clsx(
                          'text-sm text-slate-200',
                          href ? 'p-0' : 'px-4 py-3',
                          col.className,
                        )}
                      >
                        {href ? (
                          <Link
                            to={href}
                            // First cell carries the accessible label; subsequent cells are duplicates — hide from AT.
                            aria-label={colIdx === 0 ? (rowAriaLabel?.(row) ?? keyExtractor(row)) : undefined}
                            aria-hidden={colIdx !== 0 ? true : undefined}
                            tabIndex={colIdx !== 0 ? -1 : undefined}
                            className="block px-4 py-3 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500"
                          >
                            {col.render(row)}
                          </Link>
                        ) : (
                          col.render(row)
                        )}
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
