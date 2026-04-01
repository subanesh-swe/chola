import { clsx } from 'clsx';

interface Props {
  page: number;
  totalPages: number;
  onPageChange: (page: number) => void;
}

export function Pagination({ page, totalPages, onPageChange }: Props) {
  if (totalPages <= 1) return null;

  return (
    <nav className="flex items-center justify-center gap-2 mt-4" aria-label="Pagination">
      <button
        onClick={() => onPageChange(page - 1)}
        disabled={page <= 1}
        aria-label="Previous page"
        className={clsx(
          'px-3 py-1 text-sm rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500',
          page <= 1
            ? 'text-slate-600 cursor-not-allowed'
            : 'text-slate-300 hover:bg-slate-800',
        )}
      >
        Prev
      </button>
      <span className="text-sm text-slate-400" aria-live="polite" aria-atomic="true">
        {page} / {totalPages}
      </span>
      <button
        onClick={() => onPageChange(page + 1)}
        disabled={page >= totalPages}
        aria-label="Next page"
        className={clsx(
          'px-3 py-1 text-sm rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500',
          page >= totalPages
            ? 'text-slate-600 cursor-not-allowed'
            : 'text-slate-300 hover:bg-slate-800',
        )}
      >
        Next
      </button>
    </nav>
  );
}
