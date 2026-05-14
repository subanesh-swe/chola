import { useId, useRef } from 'react';
import { RELATIVE_PRESETS } from '../../utils/date';

interface Props {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  label?: string;
  className?: string;
}

/**
 * Kibana-style date expression input.
 *
 * Accepts relative expressions (now, now-1h, now-7d) and absolute ISO dates.
 * A small calendar button opens the browser's native datetime picker; selecting
 * a value from it inserts the ISO string into the text field.
 *
 * No client-side validation — the backend parses and rejects invalid values.
 */
export function DateExpressionInput({ value, onChange, placeholder, label, className }: Props) {
  const listId = useId();
  const pickerRef = useRef<HTMLInputElement>(null);

  const handlePickerChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.value) {
      onChange(e.target.value);
    }
    // Reset picker so the same value can be re-selected.
    e.target.value = '';
  };

  const openPicker = () => {
    const el = pickerRef.current;
    if (!el) return;
    if (typeof el.showPicker === 'function') {
      el.showPicker();
    } else {
      el.click();
    }
  };

  return (
    <div className={`flex flex-col gap-1 ${className ?? ''}`}>
      {label && <span className="text-xs text-muted">{label}</span>}
      <div className="flex items-center">
        <input
          type="text"
          list={listId}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder ?? 'now-1h'}
          className="w-36 bg-surface-2 border border-border-strong border-r-0 rounded-l-lg px-2 py-1.5 text-sm text-primary focus:outline-none focus:ring-1 focus:ring-accent"
          aria-label={label}
        />
        <datalist id={listId}>
          {RELATIVE_PRESETS.map((p) => (
            <option key={p} value={p} />
          ))}
        </datalist>
        {/* Hidden native datetime picker */}
        <input
          ref={pickerRef}
          type="datetime-local"
          tabIndex={-1}
          aria-hidden="true"
          className="sr-only"
          onChange={handlePickerChange}
        />
        {/* Calendar icon button */}
        <button
          type="button"
          onClick={openPicker}
          title="Pick exact date/time"
          className="bg-surface-2 border border-border-strong border-l-0 rounded-r-lg px-2 py-1.5 text-muted hover:text-primary hover:bg-surface-hover transition-colors focus:outline-none focus:ring-1 focus:ring-accent"
          aria-label="Open date picker"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
            <line x1="16" y1="2" x2="16" y2="6" />
            <line x1="8" y1="2" x2="8" y2="6" />
            <line x1="3" y1="10" x2="21" y2="10" />
          </svg>
        </button>
      </div>
    </div>
  );
}
