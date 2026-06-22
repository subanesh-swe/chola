import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

const MIN_POPOVER_WIDTH = 320;
const MAX_POPOVER_WIDTH = 720;
const VIEWPORT_MARGIN   = 16;

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
  const [activeIndex, setActiveIndex] = useState(-1);
  const btnRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const recompute = () => {
    if (!btnRef.current) return;
    const r = btnRef.current.getBoundingClientRect();
    // Use as much viewport width as possible, capped at MAX_POPOVER_WIDTH.
    const availableWidth = Math.max(
      MIN_POPOVER_WIDTH,
      Math.min(MAX_POPOVER_WIDTH, window.innerWidth - VIEWPORT_MARGIN * 2),
    );
    // Anchor: prefer right-aligned to button so the popover grows leftward
    // (avoids it shooting off the right edge of the page).
    let left = r.right + window.scrollX - availableWidth;
    // Don't let it go off the left edge either.
    if (left < window.scrollX + VIEWPORT_MARGIN) {
      left = window.scrollX + VIEWPORT_MARGIN;
    }
    setPos({
      top: r.bottom + window.scrollY + 4,
      left,
      width: availableWidth,
    });
  };

  const toggle = () => {
    if (!open) {
      recompute();
      setActiveIndex(-1);
    }
    setOpen((v) => !v);
  };

  // Close on Escape; arrow-key navigation within popover
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        btnRef.current?.focus();
        return;
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => {
          const next = Math.min(i + 1, history.length - 1);
          rowRefs.current[next]?.focus();
          return next;
        });
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => {
          const next = Math.max(i - 1, 0);
          rowRefs.current[next]?.focus();
          return next;
        });
      }
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [open, history.length]);

  // Close on outside click / touch
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent | TouchEvent) => {
      const target = (e instanceof TouchEvent ? e.touches[0]?.target : e.target) as Node | null;
      if (
        target &&
        !popoverRef.current?.contains(target) &&
        !btnRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler as EventListener);
    document.addEventListener('touchstart', handler as EventListener);
    return () => {
      document.removeEventListener('mousedown', handler as EventListener);
      document.removeEventListener('touchstart', handler as EventListener);
    };
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
            width: pos.width,
            maxWidth: `calc(100vw - ${VIEWPORT_MARGIN * 2}px)`,
            zIndex: 9999,
          }}
          className="bg-surface border border-border rounded-xl shadow-lg py-1 max-h-[60vh] overflow-y-auto"
        >
          {history.length === 0 ? (
            <p className="px-3 py-2 text-sm text-muted select-none">
              No recent queries yet
            </p>
          ) : (
            <>
              {history.map((q, idx) => (
                <button
                  key={q}
                  ref={(el) => { rowRefs.current[idx] = el; }}
                  type="button"
                  role="option"
                  aria-selected={idx === activeIndex}
                  onClick={() => handlePick(q)}
                  onFocus={() => setActiveIndex(idx)}
                  className="w-full flex items-start justify-between gap-2 px-3 py-1.5 text-left cursor-pointer hover:bg-surface-hover group focus:outline-none focus:bg-surface-hover"
                >
                  <span
                    className="text-sm text-primary font-mono flex-1 min-w-0 truncate group-hover:whitespace-pre-wrap group-hover:break-all group-focus:whitespace-pre-wrap group-focus:break-all"
                  >
                    {q}
                  </span>
                  {onRemove && (
                    <button
                      type="button"
                      onClick={(e) => handleRemove(e, q)}
                      aria-label={`Remove query from history`}
                      className="shrink-0 mt-0.5 text-muted hover:text-primary opacity-0 group-hover:opacity-100 transition-opacity focus:opacity-100 focus:outline-none rounded"
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
                </button>
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
