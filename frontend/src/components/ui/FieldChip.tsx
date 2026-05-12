import { useEffect, useRef, useState } from 'react';
import type { FieldValue } from '../../hooks/useFieldValues';

interface Props {
  field: string;
  label?: string;
  values: FieldValue[] | (() => Promise<FieldValue[]>);
  onInsert: (token: string) => void;
  freeText?: boolean;
  className?: string;
}

export function FieldChip({ field, label, values, onInsert, freeText, className }: Props) {
  const displayLabel = label ?? field;
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [resolved, setResolved] = useState<FieldValue[] | null>(
    Array.isArray(values) ? values : null,
  );
  const containerRef = useRef<HTMLDivElement>(null);

  // Close on outside click / Esc
  useEffect(() => {
    if (!open) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const onPointer = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onPointer);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', onPointer);
    };
  }, [open]);

  const handleClick = () => {
    if (loading) return;

    if (freeText) {
      onInsert(`${field}:`);
      return;
    }

    if (resolved !== null) {
      setOpen((prev) => !prev);
      return;
    }

    // Lazy-load async values
    setOpen(true);
    setLoading(true);
    (values as () => Promise<FieldValue[]>)()
      .then((v) => {
        setResolved(v);
      })
      .catch(() => {
        setResolved([]);
      })
      .finally(() => {
        setLoading(false);
      });
  };

  const selectValue = (v: FieldValue) => {
    onInsert(`${field}:${v.value}`);
    setOpen(false);
  };

  return (
    <div ref={containerRef} className={`relative ${className ?? ''}`}>
      <button
        type="button"
        onClick={handleClick}
        aria-haspopup={freeText ? undefined : 'listbox'}
        aria-expanded={freeText ? undefined : open}
        className={[
          'inline-flex items-center gap-1 px-2.5 py-1 rounded-full border text-xs font-medium transition-colors',
          'bg-surface-2 border-border-strong text-secondary',
          'hover:border-muted hover:text-primary hover:bg-surface-hover',
          'focus:outline-none focus:ring-1 focus:ring-accent',
        ].join(' ')}
      >
        {displayLabel}
        {!freeText && (
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 12 12"
            fill="currentColor"
            className="w-2.5 h-2.5 opacity-60"
            aria-hidden="true"
          >
            <path d="M6 8L1 3h10L6 8Z" />
          </svg>
        )}
      </button>

      {open && !freeText && (
        <div
          role="listbox"
          aria-label={`${displayLabel} values`}
          className={[
            'absolute left-0 top-full mt-1 z-50 min-w-[140px] max-h-60 overflow-y-auto',
            'bg-surface-2 border border-border-strong rounded-xl shadow-lg py-1',
          ].join(' ')}
        >
          {loading ? (
            <div className="flex items-center justify-center py-3 px-3">
              <svg
                className="w-4 h-4 animate-spin text-muted"
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                />
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z"
                />
              </svg>
              <span className="ml-2 text-xs text-muted">Loading…</span>
            </div>
          ) : resolved && resolved.length > 0 ? (
            resolved.map((v) => (
              <button
                key={v.value}
                type="button"
                role="option"
                aria-selected={false}
                onClick={() => selectValue(v)}
                className={[
                  'w-full flex flex-col items-start px-3 py-1.5 text-left',
                  'text-xs text-primary hover:bg-surface-hover transition-colors',
                ].join(' ')}
              >
                <span>{v.label ?? v.value}</span>
                {v.hint && <span className="text-disabled text-[10px]">{v.hint}</span>}
              </button>
            ))
          ) : (
            <p className="px-3 py-2 text-xs text-disabled">No values</p>
          )}
        </div>
      )}
    </div>
  );
}
