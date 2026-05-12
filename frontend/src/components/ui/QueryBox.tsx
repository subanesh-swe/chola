import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import type { BuildFilters } from '../../hooks/useUrlFilters';
import type { FieldValue } from '../../hooks/useFieldValues';
import type { useFieldValues } from '../../hooks/useFieldValues';
import { parseQuery, type ParseError, KNOWN_FIELDS } from '../../utils/parseQuery';
import { RecentQueriesDropdown } from './RecentQueriesDropdown';

export interface QueryBoxHandle {
  insertAtCaret: (token: string) => void;
  setValue: (v: string) => void;
  focus: () => void;
}

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (filters: Partial<BuildFilters>, rawQuery: string) => void;
  history: string[];
  onPickHistory: (q: string) => void;
  onClearHistory: () => void;
  onRemoveHistoryEntry?: (q: string) => void;
  fieldValues: ReturnType<typeof useFieldValues>;
  fields?: string[];
  placeholder?: string;
  className?: string;
}

interface SuggestionField {
  kind: 'field';
  label: string;
}

interface SuggestionValue {
  kind: 'value';
  field: string;
  item: FieldValue;
}

type Suggestion = SuggestionField | SuggestionValue;

function computeSuggestions(
  inputValue: string,
  caretPos: number,
  fields: string[],
  fieldValues: ReturnType<typeof useFieldValues>,
  resolvedValues: Record<string, FieldValue[]>,
): { suggestions: Suggestion[]; needsFetch: string | null } {
  const before = inputValue.slice(0, caretPos);

  let tokenStart = before.length;
  while (tokenStart > 0 && before[tokenStart - 1] !== ' ' && before[tokenStart - 1] !== '\t') {
    tokenStart--;
  }
  const currentToken = before.slice(tokenStart);
  const colonIdx = currentToken.indexOf(':');

  if (colonIdx === -1) {
    const partial = currentToken.toLowerCase();
    const suggestions: Suggestion[] = fields
      .filter((f) => f.startsWith(partial))
      .map((f) => ({ kind: 'field' as const, label: f }));
    return { suggestions, needsFetch: null };
  }

  const fieldName = currentToken.slice(0, colonIdx);
  const valuePrefix = currentToken.slice(colonIdx + 1).toLowerCase();

  if (!fields.includes(fieldName)) {
    return { suggestions: [], needsFetch: null };
  }

  const fieldDef = fieldValues[fieldName];
  if (!fieldDef) {
    return { suggestions: [], needsFetch: null };
  }

  if (Array.isArray(fieldDef)) {
    const matches = fieldDef.filter((fv) => fv.value.toLowerCase().startsWith(valuePrefix));
    return {
      suggestions: matches.map((item) => ({ kind: 'value' as const, field: fieldName, item })),
      needsFetch: null,
    };
  }

  if (resolvedValues[fieldName]) {
    const matches = resolvedValues[fieldName].filter((fv) =>
      fv.value.toLowerCase().startsWith(valuePrefix),
    );
    return {
      suggestions: matches.map((item) => ({ kind: 'value' as const, field: fieldName, item })),
      needsFetch: null,
    };
  }

  return { suggestions: [], needsFetch: fieldName };
}

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
    fields = Array.from(KNOWN_FIELDS),
    placeholder = 'Search: branch:main state:failed bucket:day',
    className,
  },
  ref,
) {
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const [parseError, setParseError] = useState<ParseError | null>(null);
  const [warnings, setWarnings] = useState<ParseError[]>([]);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [highlightIdx, setHighlightIdx] = useState(-1);
  const [loadingField, setLoadingField] = useState<string | null>(null);
  const [resolvedValues, setResolvedValues] = useState<Record<string, FieldValue[]>>({});
  const [showSuggestions, setShowSuggestions] = useState(false);

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
    const result = parseQuery(value);
    if (!result.ok) {
      setParseError(result.error);
      setWarnings([]);
      return;
    }
    setParseError(null);
    setWarnings(result.warnings);
    setShowSuggestions(false);
    onSubmit(result.filters, value);
  }, [value, onSubmit]);

  const recomputeSuggestions = useCallback(
    (inputValue: string, caretPos: number) => {
      const { suggestions: sugs, needsFetch } = computeSuggestions(
        inputValue,
        caretPos,
        fields,
        fieldValues,
        resolvedValues,
      );
      setSuggestions(sugs);
      setHighlightIdx(-1);

      if (needsFetch && !loadingField) {
        const fetchFn = fieldValues[needsFetch];
        if (typeof fetchFn === 'function') {
          setLoadingField(needsFetch);
          (fetchFn as () => Promise<FieldValue[]>)()
            .then((vals) => {
              setResolvedValues((prev) => ({ ...prev, [needsFetch]: vals }));
              const colonIdx = inputValue.slice(0, caretPos).lastIndexOf(':');
              const prefix = colonIdx >= 0 ? inputValue.slice(colonIdx + 1, caretPos).toLowerCase() : '';
              setSuggestions(
                vals
                  .filter((fv) => fv.value.toLowerCase().startsWith(prefix))
                  .map((item) => ({ kind: 'value' as const, field: needsFetch, item })),
              );
            })
            .catch(() => {
              setResolvedValues((prev) => ({ ...prev, [needsFetch]: [] }));
            })
            .finally(() => setLoadingField(null));
        }
      }
    },
    [fields, fieldValues, resolvedValues, loadingField],
  );

  const acceptSuggestion = useCallback(
    (sug: Suggestion) => {
      const el = inputRef.current;
      const caretPos = el?.selectionStart ?? value.length;
      const before = value.slice(0, caretPos);
      const after = value.slice(caretPos);

      let tokenStart = before.length;
      while (tokenStart > 0 && before[tokenStart - 1] !== ' ' && before[tokenStart - 1] !== '\t') {
        tokenStart--;
      }
      const prefix = before.slice(0, tokenStart);
      const replacement = sug.kind === 'field'
        ? `${sug.label}:`
        : `${sug.field}:${sug.item.value} `;

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
    const handler = (e: MouseEvent) => {
      if (
        !inputRef.current?.contains(e.target as Node) &&
        !dropdownRef.current?.contains(e.target as Node)
      ) {
        setShowSuggestions(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [showSuggestions]);

  const hasError = parseError !== null;
  const showDrop = showSuggestions && (suggestions.length > 0 || loadingField !== null);

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
            aria-label="KQL query box"
            aria-autocomplete="list"
            aria-expanded={showDrop}
            aria-describedby={hasError ? 'qbox-error' : warnings.length ? 'qbox-warnings' : undefined}
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
              {loadingField && suggestions.length === 0 ? (
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
                  <span className="text-xs text-muted">Loading values…</span>
                </div>
              ) : (
                suggestions.map((sug, idx) => {
                  const isHighlighted = idx === highlightIdx;
                  const key = sug.kind === 'field'
                    ? `field:${sug.label}`
                    : `val:${sug.field}:${sug.item.value}`;
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
                        'w-full flex flex-col items-start px-3 py-1.5 text-left transition-colors',
                        isHighlighted
                          ? 'bg-accent/15 text-primary'
                          : 'text-secondary hover:bg-surface-hover',
                      ].join(' ')}
                    >
                      {sug.kind === 'field' ? (
                        <span className="text-xs font-mono">
                          {sug.label}
                          <span className="text-disabled">:</span>
                        </span>
                      ) : (
                        <>
                          <span className="text-xs">{sug.item.label ?? sug.item.value}</span>
                          {sug.item.hint && (
                            <span className="text-[10px] text-disabled">{sug.item.hint}</span>
                          )}
                        </>
                      )}
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
        <div id="qbox-error" role="alert" className="mt-1 space-y-0.5">
          <p className="text-xs text-danger">{parseError.message}</p>
          {parseError.hint && <p className="text-xs text-disabled">{parseError.hint}</p>}
        </div>
      )}

      {!hasError && warnings.length > 0 && (
        <div id="qbox-warnings" className="mt-1 space-y-0.5">
          {warnings.map((w, i) => (
            <p key={i} className="text-xs text-warning/80" title={w.hint}>
              {w.message}
            </p>
          ))}
        </div>
      )}
    </div>
  );
});
