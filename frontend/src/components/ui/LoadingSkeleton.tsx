import { clsx } from 'clsx';

interface Props {
  rows?: number;
  className?: string;
}

export function LoadingSkeleton({ rows = 3, className }: Props) {
  return (
    <div className={clsx('space-y-3', className)}>
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="animate-pulse">
          <div className="h-4 bg-slate-800 rounded w-full" style={{ width: `${100 - i * 15}%` }} />
        </div>
      ))}
    </div>
  );
}
