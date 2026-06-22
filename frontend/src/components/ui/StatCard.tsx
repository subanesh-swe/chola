import { clsx } from 'clsx';

interface Props {
  label: string;
  value: number | string;
  color?: 'default' | 'success' | 'danger' | 'warning' | 'info';
}

const colorMap = {
  default: 'border-border',
  success: 'border-success/30',
  danger: 'border-danger/30',
  warning: 'border-warning/30',
  info: 'border-accent/30',
};

export function StatCard({ label, value, color = 'default' }: Props) {
  return (
    <div className={clsx('bg-surface border rounded-xl p-4', colorMap[color])}>
      <p className="text-sm text-muted">{label}</p>
      <p className="text-2xl font-bold text-primary mt-1">{value}</p>
    </div>
  );
}
