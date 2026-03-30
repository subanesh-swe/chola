import { clsx } from 'clsx';

interface Props {
  label: string;
  used: number;
  total: number;
  unit?: string;
  showPercentage?: boolean;
}

function getBarColor(percent: number): string {
  if (percent >= 90) return 'bg-red-500';
  if (percent >= 70) return 'bg-yellow-500';
  return 'bg-emerald-500';
}

export function ResourceBar({ label, used, total, unit = '', showPercentage = true }: Props) {
  const percent = total > 0 ? Math.min((used / total) * 100, 100) : 0;

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
