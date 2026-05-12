import { useEffect, useState, type KeyboardEvent } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { BuildFilters, Granularity } from '../../hooks/useUrlFilters';
import type { Repo } from '../../types';
import { getStageNames } from '../../api/repos';

const ALL_STATES = ['pending', 'reserved', 'running', 'success', 'failed', 'cancelled'];

const RANGE_PRESETS = [
  { label: '1h', hours: 1 },
  { label: '6h', hours: 6 },
  { label: '24h', hours: 24 },
  { label: '7d', hours: 24 * 7 },
  { label: '30d', hours: 24 * 30 },
  { label: '60d', hours: 24 * 60 },
  { label: '90d', hours: 24 * 90 },
];

const GRANULARITY_OPTIONS: Array<{ label: string; value: Granularity }> = [
  { label: 'Hour', value: 'hour' },
  { label: 'Day', value: 'day' },
  { label: 'Auto', value: 'auto' },
];

type ExitMode = 'any' | '0' | 'nonzero' | 'custom';

const EXIT_CODE_OPTIONS: Array<{ label: string; value: ExitMode }> = [
  { label: 'Any', value: 'any' },
  { label: 'Success (0)', value: '0' },
  { label: 'Non-zero', value: 'nonzero' },
  { label: 'Custom...', value: 'custom' },
];

function modeFromValue(v: string): ExitMode {
  if (v === '') return 'any';
  if (v === '0') return '0';
  if (v === 'nonzero') return 'nonzero';
  return 'custom';
}

/** Returns `YYYY-MM-DDTHH:mm` for now in local time. */
function nowLocal(): string {
  const d = new Date();
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

/** Returns `YYYY-MM-DDTHH:mm` for `hours` ago in local time. */
function hoursAgoLocal(hours: number): string {
  const d = new Date(Date.now() - hours * 3600 * 1000);
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

interface Props {
  filters: BuildFilters;
  repos: Repo[];
  onChange: (patch: Partial<BuildFilters>) => void;
  onApply: () => void;
  onReset: () => void;
  isDirty: boolean;
  isFetching?: boolean;
  /** Called with a complete patch; bypasses draft and triggers refetch immediately. */
  onPresetApply?: (patch: Partial<BuildFilters>) => void;
}

export function FilterBar({
  filters,
  repos,
  onChange,
  onApply,
  onReset,
  isDirty,
  isFetching,
  onPresetApply,
}: Props) {
  const toggleState = (s: string) => {
    const next = filters.state.includes(s)
      ? filters.state.filter((x) => x !== s)
      : [...filters.state, s];
    onChange({ state: next, page: 1 });
  };

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

  return (
    <div
      className="flex flex-col gap-3 p-3 bg-surface-2/50 border border-border rounded-xl"
      onKeyDown={onEnter}
    >
      {/* Range presets */}
      <div className="flex flex-wrap items-center gap-1">
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

      {/* Main filter row */}
      <div className="flex flex-wrap items-end gap-3">
        <StateMultiSelect selected={filters.state} onToggle={toggleState} />

        <div className="flex flex-col gap-1">
          <label className="text-xs text-muted">Repo</label>
          <select
            value={filters.repo}
            onChange={(e) => onChange({ repo: e.target.value, stage: '', page: 1 })}
            className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary min-w-[140px] focus:outline-none focus:ring-1 focus:ring-accent"
          >
            <option value="">All repos</option>
            {repos.map((r) => (
              <option key={r.id} value={r.id}>
                {r.repo_name}
              </option>
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
            className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary w-32 focus:outline-none focus:ring-1 focus:ring-accent"
          />
        </div>

        <DateRangeInputs filters={filters} onChange={onChange} />

        <StageSelect
          repoId={filters.repo}
          value={filters.stage}
          onChange={(s) => onChange({ stage: s, page: 1 })}
        />

        <ExitCodeSelect
          value={filters.exitCode}
          onChange={(v) => onChange({ exitCode: v, page: 1 })}
        />

        <GranularityToggle
          value={filters.granularity}
          onChange={(g) => onChange({ granularity: g })}
        />

        <div className="flex items-end gap-2 self-end ml-auto">
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
    </div>
  );
}

function StateMultiSelect({
  selected,
  onToggle,
}: {
  selected: string[];
  onToggle: (s: string) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">State</label>
      <div className="flex flex-wrap gap-1">
        {ALL_STATES.map((s) => (
          <button
            key={s}
            type="button"
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
        <label className="text-xs text-muted">
          From <span className="text-disabled">(UTC)</span>
        </label>
        <input
          type="datetime-local"
          value={filters.dateFrom}
          onChange={(e) => onChange({ dateFrom: e.target.value, page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary focus:outline-none focus:ring-1 focus:ring-accent"
        />
      </div>
      <div className="flex flex-col gap-1">
        <label className="text-xs text-muted">
          To <span className="text-disabled">(UTC)</span>
        </label>
        <input
          type="datetime-local"
          value={filters.dateTo}
          onChange={(e) => onChange({ dateTo: e.target.value, page: 1 })}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary focus:outline-none focus:ring-1 focus:ring-accent"
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
        className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary min-w-[120px] disabled:opacity-40 disabled:cursor-not-allowed focus:outline-none focus:ring-1 focus:ring-accent"
      >
        <option value="">All stages</option>
        {stages.map((s) => (
          <option key={s} value={s}>
            {s}
          </option>
        ))}
      </select>
    </div>
  );
}

function ExitCodeSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [mode, setMode] = useState<ExitMode>(() => modeFromValue(value));
  const [customInput, setCustomInput] = useState(modeFromValue(value) === 'custom' ? value : '');

  // Sync local state when the bound value changes from outside (URL back/forward, reset).
  useEffect(() => {
    const m = modeFromValue(value);
    setMode(m);
    if (m === 'custom') setCustomInput(value);
    else if (m === 'any') setCustomInput('');
  }, [value]);

  const handleModeChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const m = e.target.value as ExitMode;
    setMode(m);
    if (m === 'any') onChange('');
    else if (m === '0') onChange('0');
    else if (m === 'nonzero') onChange('nonzero');
    else onChange(customInput);
  };

  const handleCustomChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setCustomInput(v);
    if (mode === 'custom') onChange(v);
  };

  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">Exit code</label>
      <div className="flex gap-1">
        <select
          value={mode}
          onChange={handleModeChange}
          className="bg-surface-2 border border-border-strong rounded-lg px-3 py-1.5 text-sm text-primary min-w-[110px] focus:outline-none focus:ring-1 focus:ring-accent"
        >
          {EXIT_CODE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
        {mode === 'custom' && (
          <input
            type="number"
            value={customInput}
            onChange={handleCustomChange}
            placeholder="code"
            className="bg-surface-2 border border-border-strong rounded-lg px-2 py-1.5 text-sm text-primary w-20 focus:outline-none focus:ring-1 focus:ring-accent"
          />
        )}
      </div>
    </div>
  );
}

function GranularityToggle({
  value,
  onChange,
}: {
  value: Granularity;
  onChange: (g: Granularity) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">Granularity</label>
      <div className="flex rounded-lg overflow-hidden border border-border-strong">
        {GRANULARITY_OPTIONS.map((opt, idx) => (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            className={`text-xs px-2.5 py-1.5 transition-colors ${
              idx > 0 ? 'border-l border-border-strong' : ''
            } ${
              value === opt.value
                ? 'bg-accent text-on-accent'
                : 'bg-surface-2 text-muted hover:text-primary hover:bg-surface-hover'
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>
    </div>
  );
}
