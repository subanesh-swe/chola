import { clsx } from 'clsx';

interface Props {
  label: string;
  value: number | string;
  color?: 'default' | 'success' | 'danger' | 'warning' | 'info';
}

const colorMap = {
  default: 'border-slate-700',
  success: 'border-emerald-500/30',
  danger: 'border-red-500/30',
  warning: 'border-yellow-500/30',
  info: 'border-blue-500/30',
};

export function StatCard({ label, value, color = 'default' }: Props) {
  return (
    <div className={clsx('bg-slate-900 border rounded-xl p-4', colorMap[color])}>
      <p className="text-sm text-slate-400">{label}</p>
      <p className="text-2xl font-bold text-white mt-1">{value}</p>
    </div>
  );
}
