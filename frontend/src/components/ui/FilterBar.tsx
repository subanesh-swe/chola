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

  return (
    <div className="flex flex-wrap items-end gap-3 p-3 bg-surface-2/50 border border-border rounded-xl">
      <StateMultiSelect selected={filters.state} onToggle={toggleState} />

      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">Repo</label>
        <select
          value={filters.repo}
          onChange={(e) => onChange({ repo: e.target.value, stage: '', page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary min-w-[140px]"
        >
          <option value="">All repos</option>
          {repos.map((r) => (
            <option key={r.id} value={r.id}>{r.repo_name}</option>
          ))}
        </select>
      </div>

      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">Branch</label>
        <input
          type="text"
          value={filters.branch}
          onChange={(e) => onChange({ branch: e.target.value, page: 1 })}
          placeholder="e.g. main"
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary w-32"
        />
      </div>

      <DateRangeInputs filters={filters} onChange={onChange} />

      <StageSelect repoId={filters.repo} value={filters.stage} onChange={(s) => onChange({ stage: s, page: 1 })} />

      <ExitCodeSelect value={filters.exitCode} onChange={(v) => onChange({ exitCode: v, page: 1 })} />

      <button
        onClick={onReset}
        className="text-xs text-muted hover:text-secondary px-2 py-1.5 rounded-lg hover:bg-surface-hover transition-colors self-end"
      >
        Reset
      </button>
    </div>
  );
}

function StateMultiSelect({ selected, onToggle }: { selected: string[]; onToggle: (s: string) => void }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">State</label>
      <div className="flex flex-wrap gap-1">
        {ALL_STATES.map((s) => (
          <button
            key={s}
            onClick={() => onToggle(s)}
            className={`text-xs px-2 py-1 rounded-full border transition-colors ${
              selected.includes(s)
                ? 'bg-accent border-accent/70 text-on-accent'
                : 'bg-surface-2 border-border-strong text-muted hover:border-muted'
            }`}
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}

function DateRangeInputs({ filters, onChange }: { filters: BuildFilters; onChange: (patch: Partial<BuildFilters>) => void }) {
  return (
    <>
      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">From</label>
        <input
          type="date"
          value={filters.dateFrom}
          onChange={(e) => onChange({ dateFrom: e.target.value, page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary"
        />
      </div>
      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">To</label>
        <input
          type="date"
          value={filters.dateTo}
          onChange={(e) => onChange({ dateTo: e.target.value, page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary"
        />
      </div>
    </>
  );
}

function StageSelect({
  repoId,
  value,
  onChange,
}: {
  repoId: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const { data: stages = [] } = useQuery({
    queryKey: ['stage-names', repoId],
    queryFn: () => getStageNames(repoId),
    enabled: !!repoId,
  });

  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">Stage</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={!repoId}
        className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary min-w-[120px] disabled:opacity-40 disabled:cursor-not-allowed"
      >
        <option value="">All stages</option>
        {stages.map((s) => (
          <option key={s} value={s}>{s}</option>
        ))}
      </select>
    </div>
  );
}

function ExitCodeSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [customInput, setCustomInput] = useState('');

  const isCustom = value !== '' && value !== '0' && value !== 'nonzero';
  const selectValue = isCustom ? 'custom' : value;

  const handleSelectChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const v = e.target.value;
    if (v === 'custom') {
      onChange(customInput || '');
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
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">Exit code</label>
      <div className="flex gap-1">
        <select
          value={selectValue}
          onChange={handleSelectChange}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary min-w-[110px]"
        >
          {EXIT_CODE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
          ))}
        </select>
        {(selectValue === 'custom' || isCustom) && (
          <input
            type="number"
            value={isCustom ? value : customInput}
            onChange={handleCustomChange}
            placeholder="code"
            className="bg-surface-2 border border-border-strong rounded-lg px-2 py-1.5 text-sm text-primary w-16"
          />
        )}
      </div>
    </div>
  );
}
