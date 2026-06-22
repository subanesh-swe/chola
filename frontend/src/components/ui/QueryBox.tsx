import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import { parse } from '../../lib/chql';
import { suggestAt } from '../../lib/chql/suggest';
import type { Suggestion, FieldValueProvider } from '../../lib/chql/suggest';
import type { ValidField } from '../../lib/chql/ast';
import { VALID_FIELDS } from '../../lib/chql/ast';
import type { useFieldValues, FieldValue } from '../../hooks/useFieldValues';
import { RecentQueriesDropdown } from './RecentQueriesDropdown';

export interface QueryBoxHandle {
  insertAtCaret: (token: string) => void;
  setValue: (v: string) => void;
  focus: () => void;
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (rawQuery: string) => void;
  history: string[];
  onPickHistory: (q: string) => void;
  onClearHistory: () => void;
  onRemoveHistoryEntry?: (q: string) => void;
  fieldValues: ReturnType<typeof useFieldValues>;
  placeholder?: string;
  className?: string;
}

const KIND_BADGE_CLASSES: Record<Suggestion['kind'], string> = {
  field: 'bg-info-soft text-info',
  value: 'bg-success-soft text-success',
  operator: 'bg-warning-soft text-warning',
  keyword: 'bg-pending-soft text-pending',
};

export const QueryBox = forwardRef<QueryBoxHandle, Props>(function QueryBox(
  {
    value,
    onChange,
    onSubmit,
    history,
    onPickHistory,
    onClearHistory,
    onRemoveHistoryEntry,
    fieldValues,
    placeholder = 'Search: state:failed OR state:cancelled branch:main',
    className,
  },
  ref,
) {
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [highlightIdx, setHighlightIdx] = useState(-1);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [isLoadingSuggestions, setIsLoadingSuggestions] = useState(false);

  // Build a FieldValueProvider that bridges our useFieldValues hook format
  // to the async provider signature expected by suggestAt.
  const fieldValueProvider: FieldValueProvider = useCallback(
    async (field: ValidField): Promise<string[]> => {
      const entry = fieldValues[field as string];
      if (!entry) return [];
      if (Array.isArray(entry)) {
        return (entry as FieldValue[]).map((fv) => fv.value);
      }
      // It's a fetch function.
      try {
        const vals = await (entry as () => Promise<FieldValue[]>)();
        return vals.map((fv) => fv.value);
      } catch {
        return [];
      }
    },
    [fieldValues],
  );

  useImperativeHandle(ref, () => ({
    insertAtCaret(token: string) {
      const el = inputRef.current;
      if (!el) return;
      const pos = el.selectionStart ?? value.length;
      const before = value.slice(0, pos);
      const after = value.slice(pos);
      const needsSpace = before.length > 0 && !/\s$/.test(before);
      const insert = needsSpace ? ` ${token}` : token;
      const next = before + insert + after;
      onChange(next);
      const newPos = pos + insert.length;
      requestAnimationFrame(() => {
        el.focus();
        el.setSelectionRange(newPos, newPos);
      });
    },
    setValue(v: string) {
      onChange(v);
    },
    focus() {
      inputRef.current?.focus();
    },
  }));

  const submit = useCallback(() => {
    const result = parse(value);
    if (!result.ok) {
      const first = result.errors[0];
      setParseError(first?.message ?? 'Invalid query');
      return;
    }
    setParseError(null);
    setShowSuggestions(false);
    onSubmit(value);
  }, [value, onSubmit]);

  const recomputeSuggestions = useCallback(
    (inputValue: string, caretPos: number) => {
      setIsLoadingSuggestions(true);
      suggestAt(inputValue, caretPos, fieldValueProvider)
        .then((sugs) => {
          setSuggestions(sugs);
          setHighlightIdx(-1);
        })
        .catch(() => {
          setSuggestions([]);
        })
        .finally(() => setIsLoadingSuggestions(false));
    },
    [fieldValueProvider],
  );

  const acceptSuggestion = useCallback(
    (sug: Suggestion) => {
      const el = inputRef.current;
      const caretPos = el?.selectionStart ?? value.length;
      const before = value.slice(0, caretPos);
      const after = value.slice(caretPos);

      // Find start of the current token (non-whitespace run).
      let tokenStart = before.length;
      while (tokenStart > 0 && !/\s/.test(before[tokenStart - 1]!)) {
        tokenStart--;
      }
      const prefix = before.slice(0, tokenStart);
      const suffix = sug.insertText.endsWith(' ') ? '' : ' ';
      const replacement = sug.insertText + suffix;

      const next = prefix + replacement + after;
      onChange(next);
      const newPos = prefix.length + replacement.length;

      if (sug.kind === 'field') {
        setShowSuggestions(true);
        setSuggestions([]);
        requestAnimationFrame(() => {
          el?.focus();
          el?.setSelectionRange(newPos, newPos);
          recomputeSuggestions(next, newPos);
        });
      } else {
        setShowSuggestions(false);
        setSuggestions([]);
        setHighlightIdx(-1);
        requestAnimationFrame(() => {
          el?.focus();
          el?.setSelectionRange(newPos, newPos);
        });
      }
    },
    [value, onChange, recomputeSuggestions],
  );

  const handleChange = (v: string) => {
    if (parseError) setParseError(null);
    onChange(v);
    const el = inputRef.current;
    const caretPos = el?.selectionStart ?? v.length;
    setShowSuggestions(true);
    recomputeSuggestions(v, caretPos);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (showSuggestions && suggestions.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setHighlightIdx((i) => Math.min(i + 1, suggestions.length - 1));
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setHighlightIdx((i) => Math.max(i - 1, -1));
        return;
      }
      if (e.key === 'Escape') {
        setShowSuggestions(false);
        return;
      }
      if (e.key === 'Enter' && highlightIdx >= 0) {
        e.preventDefault();
        acceptSuggestion(suggestions[highlightIdx]);
        return;
      }
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
    if (e.key === 'Escape') {
      setShowSuggestions(false);
    }
  };

  const handlePickHistory = (q: string) => {
    onPickHistory(q);
    onChange(q);
    setShowSuggestions(false);
  };

  useEffect(() => {
    if (!showSuggestions) return;
    const handler = (e: MouseEvent | TouchEvent) => {
      const target = (e instanceof TouchEvent ? e.touches[0]?.target : e.target) as Node | null;
      if (
        target &&
        !inputRef.current?.contains(target) &&
        !dropdownRef.current?.contains(target)
      ) {
        setShowSuggestions(false);
      }
    };
    document.addEventListener('mousedown', handler as EventListener);
    document.addEventListener('touchstart', handler as EventListener);
    return () => {
      document.removeEventListener('mousedown', handler as EventListener);
      document.removeEventListener('touchstart', handler as EventListener);
    };
  }, [showSuggestions]);

  const hasError = parseError !== null;
  const showDrop = showSuggestions && (suggestions.length > 0 || isLoadingSuggestions);

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
            onFocus={() => {
              const el = inputRef.current;
              recomputeSuggestions(value, el?.selectionStart ?? value.length);
              setShowSuggestions(true);
            }}
            placeholder={placeholder}
            aria-label="ChQL query box"
            aria-autocomplete="list"
            aria-expanded={showDrop}
            aria-describedby={hasError ? 'qbox-error' : undefined}
            className={[
              'w-full bg-surface-2 border rounded-lg px-3 py-1.5 text-sm text-primary',
              'placeholder:text-disabled focus:outline-none focus:ring-1',
              hasError
                ? 'border-danger/70 focus:ring-danger/50'
                : 'border-border-strong focus:ring-accent',
            ].join(' ')}
          />

          {showDrop && (
            <div
              ref={dropdownRef}
              role="listbox"
              aria-label="Query suggestions"
              className={[
                'absolute left-0 top-full mt-1 z-50 w-full max-h-52 overflow-y-auto',
                'bg-surface-2 border border-border-strong rounded-xl shadow-lg py-1',
              ].join(' ')}
            >
              {isLoadingSuggestions && suggestions.length === 0 ? (
                <div className="flex items-center gap-2 px-3 py-2">
                  <svg
                    className="w-3.5 h-3.5 animate-spin text-muted"
                    xmlns="http://www.w3.org/2000/svg"
                    fill="none"
                    viewBox="0 0 24 24"
                    aria-hidden="true"
                  >
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z" />
                  </svg>
                  <span className="text-xs text-muted">Loading suggestions...</span>
                </div>
              ) : (
                suggestions.map((sug, idx) => {
                  const isHighlighted = idx === highlightIdx;
                  const key = `${sug.kind}:${sug.label}`;
                  return (
                    <button
                      key={key}
                      type="button"
                      role="option"
                      aria-selected={isHighlighted}
                      onMouseDown={(e) => {
                        e.preventDefault();
                        acceptSuggestion(sug);
                      }}
                      onMouseEnter={() => setHighlightIdx(idx)}
                      className={[
                        'w-full flex items-center gap-2 px-3 py-1.5 text-left transition-colors',
                        isHighlighted
                          ? 'bg-accent/15 text-primary'
                          : 'text-secondary hover:bg-surface-hover',
                      ].join(' ')}
                    >
                      <span className="text-xs font-mono flex-1 truncate">{sug.label}</span>
                      {sug.detail && (
                        <span className="text-[10px] text-disabled truncate max-w-[80px]">{sug.detail}</span>
                      )}
                      <span className={`text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase shrink-0 ${KIND_BADGE_CLASSES[sug.kind]}`}>
                        {sug.kind}
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          )}
        </div>

        <button
          type="button"
          onClick={submit}
          aria-label="Run query"
          className="shrink-0 px-2 py-1.5 bg-surface-2 border border-border-strong rounded-lg text-muted hover:text-primary hover:border-muted transition-colors focus:outline-none focus:ring-1 focus:ring-accent"
        >
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

        <RecentQueriesDropdown
          history={history}
          onPick={handlePickHistory}
          onClear={onClearHistory}
          onRemove={onRemoveHistoryEntry}
        />
      </div>

      {hasError && (
        <div id="qbox-error" role="alert" className="mt-1">
          <p className="text-xs text-danger">{parseError}</p>
        </div>
      )}

      <div className="mt-1 flex flex-wrap gap-1">
        {VALID_FIELDS.map((f) => (
          <span
            key={f}
            className="text-[10px] px-1.5 py-0.5 rounded bg-surface border border-border text-disabled font-mono cursor-default"
            title={`Field: ${f}`}
          >
            {f}:
          </span>
        ))}
      </div>
    </div>
  );
});
