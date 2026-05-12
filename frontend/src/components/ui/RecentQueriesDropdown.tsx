import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

const MAX_DISPLAY_LEN = 60;

function truncate(s: string): string {
  return s.length <= MAX_DISPLAY_LEN ? s : s.slice(0, MAX_DISPLAY_LEN - 1) + '…';
}

interface Props {
  history: string[];
  onPick: (q: string) => void;
  onClear: () => void;
  onRemove?: (q: string) => void;
  className?: string;
}

interface PopoverPos {
  top: number;
  left: number;
  width: number;
}

export function RecentQueriesDropdown({ history, onPick, onClear, onRemove, className }: Props) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<PopoverPos>({ top: 0, left: 0, width: 200 });
  const btnRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  const recompute = () => {
    if (!btnRef.current) return;
    const r = btnRef.current.getBoundingClientRect();
    setPos({
      top: r.bottom + window.scrollY + 4,
      left: r.left + window.scrollX,
      width: Math.max(r.width, 280),
    });
  };

  const toggle = () => {
    if (!open) recompute();
    setOpen((v) => !v);
  };

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        btnRef.current?.focus();
      }
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [open]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        !popoverRef.current?.contains(target) &&
        !btnRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  // Reposition on scroll/resize
  useEffect(() => {
    if (!open) return;
    const handler = () => recompute();
    window.addEventListener('scroll', handler, true);
    window.addEventListener('resize', handler);
    return () => {
      window.removeEventListener('scroll', handler, true);
      window.removeEventListener('resize', handler);
    };
  }, [open]);

  const handlePick = (q: string) => {
    onPick(q);
    setOpen(false);
  };

  const handleRemove = (e: React.MouseEvent, q: string) => {
    e.stopPropagation();
    onRemove?.(q);
  };

  const handleClear = () => {
    onClear();
    setOpen(false);
  };

  const popover = open
    ? createPortal(
        <div
          ref={popoverRef}
          role="listbox"
          aria-label="Recent queries"
          style={{
            position: 'absolute',
            top: pos.top,
            left: pos.left,
            minWidth: pos.width,
            zIndex: 9999,
          }}
          className="bg-surface border border-border rounded-xl shadow-lg py-1 max-h-72 overflow-y-auto"
        >
          {history.length === 0 ? (
            <p className="px-3 py-2 text-sm text-muted select-none">
              No recent queries yet
            </p>
          ) : (
            <>
              {history.map((q) => (
                <div
                  key={q}
                  role="option"
                  aria-selected={false}
                  onClick={() => handlePick(q)}
                  className="flex items-center justify-between gap-2 px-3 py-1.5 cursor-pointer hover:bg-surface-hover group"
                >
                  <span
                    className="text-sm text-primary truncate flex-1"
                    title={q}
                  >
                    {truncate(q)}
                  </span>
                  {onRemove && (
                    <button
                      type="button"
                      onClick={(e) => handleRemove(e, q)}
                      aria-label={`Remove "${truncate(q)}" from history`}
                      className="shrink-0 text-muted hover:text-primary opacity-0 group-hover:opacity-100 transition-opacity focus:opacity-100 focus:outline-none rounded"
                    >
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        viewBox="0 0 16 16"
                        fill="currentColor"
                        className="w-3.5 h-3.5"
                        aria-hidden="true"
                      >
                        <path d="M5.28 4.22a.75.75 0 0 0-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 1 0 1.06 1.06L8 9.06l2.72 2.72a.75.75 0 1 0 1.06-1.06L9.06 8l2.72-2.72a.75.75 0 0 0-1.06-1.06L8 6.94 5.28 4.22Z" />
                      </svg>
                    </button>
                  )}
                </div>
              ))}
              <div className="border-t border-border mt-1 pt-1">
                <button
                  type="button"
                  onClick={handleClear}
                  className="w-full text-left px-3 py-1.5 text-xs text-muted hover:text-primary hover:bg-surface-hover transition-colors focus:outline-none focus:ring-1 focus:ring-accent rounded"
                >
                  Clear all
                </button>
              </div>
            </>
          )}
        </div>,
        document.body,
      )
    : null;

  return (
    <div className={className}>
      <button
        ref={btnRef}
        type="button"
        onClick={toggle}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="Show recent queries"
        className="flex items-center gap-1 px-2 py-1.5 text-sm text-muted hover:text-primary bg-surface-2 border border-border rounded-lg hover:bg-surface-hover transition-colors focus:outline-none focus:ring-1 focus:ring-accent"
      >
        {/* History clock icon */}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          fill="currentColor"
          className="w-3.5 h-3.5"
          aria-hidden="true"
        >
          <path
            fillRule="evenodd"
            d="M1 8a7 7 0 1 1 14 0A7 7 0 0 1 1 8Zm7.75-4.25a.75.75 0 0 0-1.5 0V8c0 .414.336.75.75.75h3a.75.75 0 0 0 0-1.5h-2.25V3.75Z"
            clipRule="evenodd"
          />
        </svg>
        <span>Recent</span>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          fill="currentColor"
          className="w-3 h-3 opacity-60"
          aria-hidden="true"
        >
          <path
            fillRule="evenodd"
            d="M4.22 6.22a.75.75 0 0 1 1.06 0L8 8.94l2.72-2.72a.75.75 0 1 1 1.06 1.06l-3.25 3.25a.75.75 0 0 1-1.06 0L4.22 7.28a.75.75 0 0 1 0-1.06Z"
            clipRule="evenodd"
          />
        </svg>
      </button>
      {popover}
    </div>
  );
}
