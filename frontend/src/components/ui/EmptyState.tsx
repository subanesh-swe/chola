import type { ReactNode } from 'react';

interface Props {
  title: string;
  description?: string;
  icon?: ReactNode;
  action?: ReactNode;
}

export function EmptyState({ title, description, icon, action }: Props) {
  return (
    <div className="flex flex-col items-center justify-center py-16 px-6 text-center bg-slate-900/50 border border-slate-700/50 rounded-xl">
      {icon && (
        <div className="mb-4 text-slate-500" aria-hidden="true">
          {icon}
        </div>
      )}
      <p className="text-base font-medium text-slate-300">{title}</p>
      {description && (
        <p className="mt-1 text-sm text-slate-500 max-w-sm">{description}</p>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
