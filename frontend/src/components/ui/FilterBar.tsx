import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { BuildFilters } from '../../hooks/useUrlFilters';
import type { Repo } from '../../types';
import { getStageNames } from '../../api/repos';

const ALL_STATES = ['pending', 'reserved', 'running', 'success', 'failed', 'cancelled'];

const EXIT_CODE_OPTIONS = [
  { label: 'Any', value: '' },
  { label: 'Success (0)', value: '0' },
  { label: 'Non-zero', value: 'nonzero' },
  { label: 'Custom...', value: 'custom' },
];

interface Props {
  filters: BuildFilters;
  repos: Repo[];
  onChange: (patch: Partial<BuildFilters>) => void;
  onReset: () => void;
}

export function FilterBar({ filters, repos, onChange, onReset }: Props) {
  const toggleState = (s: string) => {
    const next = filters.state.includes(s)
      ? filters.state.filter((x) => x !== s)
      : [...filters.state, s];
    onChange({ state: next, page: 1 });
  };

  return (
    <div className="flex flex-wrap items-end gap-3 p-3 bg-slate-800/50 border border-slate-700 rounded-xl">
      <StateMultiSelect selected={filters.state} onToggle={toggleState} />

      <div className="flex flex-col gap-1">
        <label className="text-xs text-slate-400">Repo</label>
        <select
          value={filters.repo}
          onChange={(e) => onChange({ repo: e.target.value, stage: '', page: 1 })}
          className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white min-w-[140px]"
        >
          <option value="">All repos</option>
          {repos.map((r) => (
            <option key={r.id} value={r.id}>{r.repo_name}</option>
          ))}
        </select>
      </div>

      <div className="flex flex-col gap-1">
        <label className="text-xs text-slate-400">Branch</label>
        <input
          type="text"
          value={filters.branch}
          onChange={(e) => onChange({ branch: e.target.value, page: 1 })}
          placeholder="e.g. main"
          className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white w-32"
        />
      </div>

      <DateRangeInputs filters={filters} onChange={onChange} />

      <StageSelect repoId={filters.repo} value={filters.stage} onChange={(s) => onChange({ stage: s, page: 1 })} />

      <ExitCodeSelect value={filters.exitCode} onChange={(v) => onChange({ exitCode: v, page: 1 })} />

      <button
        onClick={onReset}
        className="text-xs text-slate-400 hover:text-slate-200 px-2 py-1.5 rounded-lg hover:bg-slate-700 transition-colors self-end"
      >
        Reset
      </button>
    </div>
  );
}

function StateMultiSelect({ selected, onToggle }: { selected: string[]; onToggle: (s: string) => void }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-slate-400">State</label>
      <div className="flex flex-wrap gap-1">
        {ALL_STATES.map((s) => (
          <button
            key={s}
            onClick={() => onToggle(s)}
            className={`text-xs px-2 py-1 rounded-full border transition-colors ${
              selected.includes(s)
                ? 'bg-indigo-600 border-indigo-500 text-white'
                : 'bg-slate-800 border-slate-600 text-slate-400 hover:border-slate-400'
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
        <label className="text-xs text-slate-400">From</label>
        <input
          type="date"
          value={filters.dateFrom}
          onChange={(e) => onChange({ dateFrom: e.target.value, page: 1 })}
          className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white"
        />
      </div>
      <div className="flex flex-col gap-1">
        <label className="text-xs text-slate-400">To</label>
        <input
          type="date"
          value={filters.dateTo}
          onChange={(e) => onChange({ dateTo: e.target.value, page: 1 })}
          className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white"
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
      <label className="text-xs text-slate-400">Stage</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={!repoId}
        className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white min-w-[120px] disabled:opacity-40 disabled:cursor-not-allowed"
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
      onChange(v);
    }
  };

  const handleCustomChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setCustomInput(v);
    onChange(v);
  };

  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-slate-400">Exit code</label>
      <div className="flex gap-1">
        <select
          value={selectValue}
          onChange={handleSelectChange}
          className="bg-slate-800 border border-slate-600 rounded-lg px-3 py-1.5 text-sm text-white min-w-[110px]"
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
            className="bg-slate-800 border border-slate-600 rounded-lg px-2 py-1.5 text-sm text-white w-16"
          />
        )}
      </div>
    </div>
  );
}
