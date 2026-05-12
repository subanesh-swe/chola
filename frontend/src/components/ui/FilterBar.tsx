import { useRef, type KeyboardEvent } from 'react';
import type { BuildFilters } from '../../hooks/useUrlFilters';
import { useFieldValues } from '../../hooks/useFieldValues';
import type { useQueryHistory } from '../../hooks/useQueryHistory';
import { nowLocal, hoursAgoLocal } from '../../utils/date';
import { QueryBox, type QueryBoxHandle } from './QueryBox';
import { FieldChipsRow } from './FieldChipsRow';

const RANGE_PRESETS = [
  { label: '1h', hours: 1 },
  { label: '6h', hours: 6 },
  { label: '24h', hours: 24 },
  { label: '7d', hours: 24 * 7 },
  { label: '30d', hours: 24 * 30 },
  { label: '60d', hours: 24 * 60 },
  { label: '90d', hours: 24 * 90 },
];

type HideableField = 'dateRange' | 'rangePresets';

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
}: Props) {
  const hide = (f: HideableField) => hiddenFields.includes(f);
  const queryRef = useRef<QueryBoxHandle>(null);
  const fieldValues = useFieldValues();

  const onEnter = (e: KeyboardEvent) => {
    if (e.key === 'Enter') onApply();
  };

  const applyPreset = (hours: number) => {
    const patch: Partial<BuildFilters> = {
      dateFrom: hoursAgoLocal(hours),
      dateTo: nowLocal(),
      page: 1,
    };
    if (onPresetApply) {
      onPresetApply(patch);
    } else {
      onChange(patch);
      onApply();
    }
  };

  const handleQuerySubmit = (parsed: Partial<BuildFilters>, rawQuery: string) => {
    historyApi.push(rawQuery);
    if (onPresetApply) {
      onPresetApply({ ...parsed, page: 1 });
    } else {
      onChange({ ...parsed, page: 1 });
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
      {/* Date range + presets */}
      {!hide('dateRange') && (
        <div className="flex flex-wrap items-end gap-3">
          <DateRangeInputs filters={filters} onChange={onChange} />

          {!hide('rangePresets') && (
            <div className="flex flex-wrap items-center gap-1 self-end pb-0.5">
              <span className="text-xs text-muted mr-1">Range:</span>
              {RANGE_PRESETS.map((preset) => (
                <button
                  key={preset.label}
                  type="button"
                  onClick={() => applyPreset(preset.hours)}
                  className="text-xs px-2 py-0.5 rounded border border-border-strong text-muted hover:text-primary hover:border-muted transition-colors"
                >
                  {preset.label}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

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

      {/* Actions */}
      <div className="flex items-center gap-2 justify-end">
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
    </div>
  );
}

function DateRangeInputs({
  filters,
  onChange,
}: {
  filters: BuildFilters;
  onChange: (patch: Partial<BuildFilters>) => void;
}) {
  return (
    <>
      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">From</label>
        <input
          type="datetime-local"
          value={filters.dateFrom}
          onChange={(e) => onChange({ dateFrom: e.target.value, page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary focus:outline-none focus:ring-1 focus:ring-accent"
        />
      </div>
      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">To</label>
        <input
          type="datetime-local"
          value={filters.dateTo}
          onChange={(e) => onChange({ dateTo: e.target.value, page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary focus:outline-none focus:ring-1 focus:ring-accent"
        />
        <span className="text-xs text-disabled">Times are local; the server normalizes to UTC.</span>
      </div>
    </>
  );
}
