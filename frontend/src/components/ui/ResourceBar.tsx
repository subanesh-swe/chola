interface Props {
  label: string;
  total: number;
  reserved: number;
  used: number;
  unit: string;
  /** When true, `used` is already a percentage (0-100) rather than an absolute value */
  usedIsPercent?: boolean;
}

function getBarColor(percent: number): string {
  if (percent >= 90) return 'bg-red-500';
  if (percent >= 70) return 'bg-yellow-500';
  return 'bg-emerald-500';
}

export function ResourceBar({
  label,
  total,
  reserved,
  used,
  unit,
  usedIsPercent = false,
}: Props) {
  const clamp = (v: number) => Math.max(0, Math.min(v, 100));
  const safe = (v: number) => (isNaN(v) || v < 0 ? 0 : v);
  const reservedPct = total > 0 ? clamp((safe(reserved) / total) * 100) : 0;
  const usedPct = usedIsPercent ? clamp(safe(used)) : (total > 0 ? clamp((safe(used) / total) * 100) : 0);

  return (
    <div className="space-y-1.5">
      <div className="text-xs text-slate-400 font-medium">{label} ({total} {unit})</div>

      {/* Reserved bar */}
      <div className="flex items-center gap-2">
        <span className="text-[10px] text-slate-500 w-14 shrink-0">Reserved</span>
        <div className="flex-1 h-2 bg-slate-700 rounded-full overflow-hidden">
          {reservedPct > 0 && (
            <div
              className="h-full bg-indigo-500 rounded-full transition-all duration-500"
              style={{ width: `${reservedPct}%` }}
            />
          )}
        </div>
        <span className="text-[10px] text-slate-400 w-24 text-right shrink-0">
          {reserved} / {total} {unit}
        </span>
      </div>

      {/* Usage bar */}
      <div className="flex items-center gap-2">
        <span className="text-[10px] text-slate-500 w-14 shrink-0">Usage</span>
        <div className="flex-1 h-2 bg-slate-700 rounded-full overflow-hidden">
          {usedPct > 0 && (
            <div
              className={`h-full rounded-full transition-all duration-500 ${getBarColor(usedPct)}`}
              style={{ width: `${usedPct}%` }}
            />
          )}
        </div>
        <span className="text-[10px] text-slate-400 w-24 text-right shrink-0">
          {usedIsPercent
            ? `${safe(used).toFixed(0)}%`
            : `${safe(used).toLocaleString()} / ${total.toLocaleString()} ${unit}`}
        </span>
      </div>
    </div>
  );
}
