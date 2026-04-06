import { clsx } from 'clsx';

interface Props {
  label: string;
  used: number;
  total: number;
  unit?: string;
  showPercentage?: boolean;
  /** Portion reserved by active builds (same unit as used/total). When provided,
   *  the bar renders three segments: allocated (indigo) | used (green) | free (slate). */
  allocated?: number;
}

function getBarColor(percent: number): string {
  if (percent >= 90) return 'bg-red-500';
  if (percent >= 70) return 'bg-yellow-500';
  return 'bg-emerald-500';
}

export function ResourceBar({
  label,
  used,
  total,
  unit = '',
  showPercentage = true,
  allocated,
}: Props) {
  const percent = total > 0 ? Math.min((used / total) * 100, 100) : 0;

  if (allocated !== undefined) {
    const allocPct = total > 0 ? Math.min((allocated / total) * 100, 100) : 0;
    // Used segment sits on top of allocated; clamp combined to 100%
    const usedPct = total > 0 ? Math.min((used / total) * 100, 100 - allocPct) : 0;
    const totalOccupied = allocPct + usedPct;

    return (
      <div className="space-y-1">
        <div className="flex justify-between text-xs">
          <span className="text-slate-400">{label}</span>
          <span className="text-slate-300">
            {used.toLocaleString()}{unit} / {total.toLocaleString()}{unit}
            {showPercentage && (
              <span className="text-slate-500 ml-1">({percent.toFixed(0)}%)</span>
            )}
            {allocated > 0 && (
              <span className="text-indigo-400 ml-1">
                ({allocated.toLocaleString()}{unit} reserved)
              </span>
            )}
          </span>
        </div>
        <div className="h-2 bg-slate-700 rounded-full overflow-hidden flex">
          {allocPct > 0 && (
            <div
              className="h-full bg-indigo-500 transition-all duration-500 shrink-0"
              style={{ width: `${allocPct}%` }}
              title={`Reserved: ${allocated.toLocaleString()}${unit}`}
            />
          )}
          {usedPct > 0 && (
            <div
              className={clsx('h-full transition-all duration-500 shrink-0', getBarColor(totalOccupied))}
              style={{ width: `${usedPct}%` }}
              title={`Used: ${used.toLocaleString()}${unit}`}
            />
          )}
        </div>
        {(allocPct > 0 || usedPct > 0) && (
          <div className="flex gap-3 text-[10px] text-slate-500">
            {allocPct > 0 && (
              <span className="flex items-center gap-1">
                <span className="inline-block w-2 h-2 rounded-sm bg-indigo-500" />
                Reserved
              </span>
            )}
            {usedPct > 0 && (
              <span className="flex items-center gap-1">
                <span className={clsx('inline-block w-2 h-2 rounded-sm', getBarColor(totalOccupied))} />
                Used
              </span>
            )}
            <span className="flex items-center gap-1">
              <span className="inline-block w-2 h-2 rounded-sm bg-slate-700" />
              Free
            </span>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <div className="flex justify-between text-xs">
        <span className="text-slate-400">{label}</span>
        <span className="text-slate-300">
          {used.toLocaleString()}{unit} / {total.toLocaleString()}{unit}
          {showPercentage && (
            <span className="text-slate-500 ml-1">({percent.toFixed(0)}%)</span>
          )}
        </span>
      </div>
      <div className="h-2 bg-slate-700 rounded-full overflow-hidden">
        <div
          className={clsx('h-full rounded-full transition-all duration-500', getBarColor(percent))}
          style={{ width: `${percent}%` }}
        />
      </div>
    </div>
  );
}
