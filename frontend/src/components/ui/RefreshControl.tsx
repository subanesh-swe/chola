const INTERVALS = [
  { label: 'Off', secs: 0 },
  { label: '5s', secs: 5 },
  { label: '10s', secs: 10 },
  { label: '30s', secs: 30 },
  { label: '1m', secs: 60 },
  { label: '5m', secs: 300 },
];

interface Props {
  intervalSecs: number;
  onIntervalChange: (s: number) => void;
  onRefresh: () => void;
  isFetching?: boolean;
}

export function RefreshControl({ intervalSecs, onIntervalChange, onRefresh, isFetching }: Props) {
  return (
    <div className="flex items-center gap-1 bg-surface border border-border rounded-lg p-0.5">
      <button
        type="button"
        onClick={onRefresh}
        title="Refresh now"
        className="px-2 py-1 text-sm rounded text-muted hover:text-primary hover:bg-surface-hover transition-colors"
      >
        <span className={`inline-block ${isFetching ? 'animate-spin' : ''}`}>↻</span>
      </button>
      <select
        value={intervalSecs}
        onChange={(e) => onIntervalChange(Number(e.target.value))}
        title="Auto-refresh interval"
        className="bg-transparent text-xs px-2 py-1 text-muted border-l border-border focus:outline-none"
      >
        {INTERVALS.map((i) => (
          <option key={i.secs} value={i.secs} className="bg-surface">
            {i.label}
          </option>
        ))}
      </select>
    </div>
  );
}
