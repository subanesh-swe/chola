import { type KeyboardEvent, useRef, useState } from 'react';
import type { BuildFilters } from '../../hooks/useUrlFilters';
import { parseQuery, type ParseError } from '../../utils/parseQuery';

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (filters: Partial<BuildFilters>) => void;
  className?: string;
}

export function QueryBox({ value, onChange, onSubmit, className }: Props) {
  const [parseError, setParseError] = useState<ParseError | null>(null);
  const [warnings, setWarnings] = useState<ParseError[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const submit = () => {
    const result = parseQuery(value);
    if (!result.ok) {
      setParseError(result.error);
      setWarnings([]);
      return;
    }
    setParseError(null);
    setWarnings(result.warnings);
    onSubmit(result.filters);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  };

  const handleChange = (v: string) => {
    // Clear error immediately as the user edits; don't re-parse on every keystroke.
    if (parseError) setParseError(null);
    onChange(v);
  };

  const hasError = parseError !== null;

  return (
    <div className={className}>
      <div className="flex items-center gap-1">
        <div className="relative flex-1">
          <input
            ref={inputRef}
            type="text"
            value={value}
            onChange={(e) => handleChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search: branch:main state:failed exit_code:!=0"
            aria-label="KQL query box"
            aria-describedby={hasError ? 'qbox-error' : warnings.length ? 'qbox-warnings' : undefined}
            className={[
              'w-full bg-surface-2 border rounded-lg px-3 py-1.5 text-sm text-primary',
              'placeholder:text-disabled focus:outline-none focus:ring-1',
              hasError
                ? 'border-red-500/70 focus:ring-red-500/50'
                : 'border-border-strong focus:ring-accent',
            ].join(' ')}
          />
        </div>

        <button
          type="button"
          onClick={submit}
          aria-label="Run query"
          className="shrink-0 px-2 py-1.5 bg-surface-2 border border-border-strong rounded-lg text-muted hover:text-primary hover:border-muted transition-colors focus:outline-none focus:ring-1 focus:ring-accent"
        >
          {/* magnifier icon using SVG so no emoji dependency */}
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 20 20"
            fill="currentColor"
            className="w-4 h-4"
            aria-hidden="true"
          >
            <path
              fillRule="evenodd"
              d="M9 3.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11ZM2 9a7 7 0 1 1 12.452 4.391l3.328 3.329a.75.75 0 1 1-1.06 1.06l-3.329-3.328A7 7 0 0 1 2 9Z"
              clipRule="evenodd"
            />
          </svg>
        </button>
      </div>

      {hasError && (
        <div id="qbox-error" role="alert" className="mt-1 space-y-0.5">
          <p className="text-xs text-red-400">{parseError.message}</p>
          {parseError.hint && (
            <p className="text-xs text-disabled">{parseError.hint}</p>
          )}
        </div>
      )}

      {!hasError && warnings.length > 0 && (
        <div id="qbox-warnings" className="mt-1 space-y-0.5">
          {warnings.map((w, i) => (
            <p key={i} className="text-xs text-amber-400/80" title={w.hint}>
              {w.message}
            </p>
          ))}
        </div>
      )}
    </div>
  );
}
