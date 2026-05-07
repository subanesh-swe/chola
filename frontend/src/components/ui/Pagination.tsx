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
          'px-3 py-1 text-sm rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent',
          page <= 1
            ? 'text-disabled cursor-not-allowed'
            : 'text-secondary hover:bg-surface-hover',
        )}
      >
        Prev
      </button>
      <span className="text-sm text-muted" aria-live="polite" aria-atomic="true">
        {page} / {totalPages}
      </span>
      <button
        onClick={() => onPageChange(page + 1)}
        disabled={page >= totalPages}
        aria-label="Next page"
        className={clsx(
          'px-3 py-1 text-sm rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent',
          page >= totalPages
            ? 'text-disabled cursor-not-allowed'
            : 'text-secondary hover:bg-surface-hover',
        )}
      >
        Next
      </button>
    </nav>
  );
}
