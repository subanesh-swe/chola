export function TableSkeleton({ rows = 5, cols = 4 }: { rows?: number; cols?: number }) {
  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
      <div className="border-b border-slate-700 px-4 py-3 flex gap-4">
        {Array.from({ length: cols }).map((_, i) => (
          <div
            key={i}
            className="h-3 bg-slate-800 rounded animate-pulse"
            style={{ width: `${60 + (i * 17 + 13) % 40}px` }}
          />
        ))}
      </div>
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="px-4 py-3 flex gap-4 border-b border-slate-800 last:border-0">
          {Array.from({ length: cols }).map((_, j) => (
            <div
              key={j}
              className="h-4 bg-slate-800 rounded animate-pulse"
              style={{ width: `${40 + ((i * cols + j) * 23 + 7) % 80}px` }}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

export function DashboardSkeleton() {
  return (
    <div className="space-y-6">
      <div className="grid grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="bg-slate-900 border border-slate-700 rounded-xl p-4 animate-pulse">
            <div className="h-3 w-20 bg-slate-800 rounded mb-2" />
            <div className="h-7 w-16 bg-slate-800 rounded" />
          </div>
        ))}
      </div>
      <div className="grid grid-cols-2 gap-6">
        <TableSkeleton rows={5} cols={3} />
        <TableSkeleton rows={5} cols={3} />
      </div>
    </div>
  );
}
