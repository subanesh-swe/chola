import { formatDistanceToNow } from 'date-fns';

interface Props {
  date: string;
  className?: string;
}

export function TimeAgo({ date, className }: Props) {
  const formatted = formatDistanceToNow(new Date(date), { addSuffix: true });
  return (
    <span className={className} title={new Date(date).toLocaleString()}>
      {formatted}
    </span>
  );
}
