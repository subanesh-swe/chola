import { useRef, type KeyboardEvent } from 'react';
import type { BuildFilters } from '../../hooks/useUrlFilters';
import { useFieldValues } from '../../hooks/useFieldValues';
import type { useQueryHistory } from '../../hooks/useQueryHistory';
import { QueryBox, type QueryBoxHandle } from './QueryBox';
import { FieldChipsRow } from './FieldChipsRow';
import { DateExpressionInput } from './DateExpressionInput';
import { RefreshControl } from './RefreshControl';

type HideableField = 'dateRange';

interface Props {
  filters: BuildFilters;
  queryValue: string;
  onQueryChange: (v: string) => void;
  onChange: (patch: Partial<BuildFilters>) => void;
  /** Called when Search is clicked or Enter is pressed. */
  onApply?: () => void;
  onReset: () => void;
  isDirty?: boolean;
  isFetching?: boolean;
  /** Called with a complete patch; bypasses draft and triggers refetch immediately. */
  onPresetApply?: (patch: Partial<BuildFilters>) => void;
  /** Fields to hide — lets queue / other pages show a trimmed filter set. */
  hiddenFields?: HideableField[];
  historyApi: ReturnType<typeof useQueryHistory>;
  /** Refresh control — provide all three or none. */
  refreshSecs?: number;
  onIntervalChange?: (s: number) => void;
  onRefresh?: () => void;
}

export function FilterBar({
  filters,
  queryValue,
  onQueryChange,
  onChange,
  onApply = () => undefined,
  onReset,
  isDirty = false,
  isFetching,
  onPresetApply,
  hiddenFields = [],
  historyApi,
  refreshSecs,
  onIntervalChange,
  onRefresh,
}: Props) {
  const hide = (f: HideableField) => hiddenFields.includes(f);
  const queryRef = useRef<QueryBoxHandle>(null);
  const fieldValues = useFieldValues();

  const showDates = !hide('dateRange');
  const showRefresh =
    refreshSecs !== undefined && onIntervalChange !== undefined && onRefresh !== undefined;

  const onEnter = (e: KeyboardEvent) => {
    if (e.key === 'Enter') onApply();
  };

  const handleQuerySubmit = (rawQuery: string) => {
    historyApi.push(rawQuery);
    const patch: Partial<BuildFilters> = { q: rawQuery, page: 1 };
    if (onPresetApply) {
      onPresetApply(patch);
    } else {
      onChange(patch);
      onApply();
    }
  };

  const handlePickHistory = (q: string) => {
    onQueryChange(q);
  };

  const handleInsertChip = (token: string) => {
    queryRef.current?.insertAtCaret(token);
  };

  return (
    <div
      className="flex flex-col gap-3 p-3 bg-surface-2/50 border border-border rounded-xl"
      onKeyDown={onEnter}
    >
      {/* Query box */}
      <QueryBox
        ref={queryRef}
        value={queryValue}
        onChange={onQueryChange}
        onSubmit={handleQuerySubmit}
        history={historyApi.history}
        onPickHistory={handlePickHistory}
        onClearHistory={historyApi.clear}
        onRemoveHistoryEntry={historyApi.remove}
        fieldValues={fieldValues}
      />

      {/* Field chips for quick token insertion */}
      <FieldChipsRow onInsert={handleInsertChip} />

      {/* Bottom row: Search/Reset left | date expressions + refresh right */}
      <div className="flex items-end justify-between flex-wrap gap-3">
        {/* Left: action buttons */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onReset}
            className="text-xs text-muted hover:text-secondary px-2 py-1.5 rounded-lg hover:bg-surface-hover transition-colors"
          >
            Reset
          </button>
          <button
            type="button"
            onClick={onApply}
            disabled={!isDirty}
            className={`text-xs font-medium px-3 py-1.5 rounded-lg transition-colors ${
              !isDirty
                ? 'bg-surface-2 text-disabled cursor-not-allowed'
                : isFetching
                  ? 'bg-accent text-on-accent animate-pulse'
                  : 'bg-accent hover:bg-accent/80 text-on-accent'
            }`}
          >
            Search
          </button>
        </div>

        {/* Right: date range + refresh control */}
        {(showDates || showRefresh) && (
          <div className="flex items-end gap-3 flex-wrap">
            {showDates && (
              <>
                <DateExpressionInput
                  label="From"
                  value={filters.dateFrom}
                  onChange={(v) => onChange({ dateFrom: v, page: 1 })}
                  placeholder="now-7d"
                />
                <DateExpressionInput
                  label="To"
                  value={filters.dateTo}
                  onChange={(v) => onChange({ dateTo: v, page: 1 })}
                  placeholder="now"
                />
              </>
            )}
            {showRefresh && (
              <div className="pb-0.5">
                <RefreshControl
                  intervalSecs={refreshSecs!}
                  onIntervalChange={onIntervalChange!}
                  onRefresh={onRefresh!}
                  isFetching={isFetching}
                />
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
