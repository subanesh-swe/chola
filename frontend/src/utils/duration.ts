export function durationMs(start: string | null, end: string | null): number {
  if (!start) return 0;
  return (end ? new Date(end).getTime() : Date.now()) - new Date(start).getTime();
}

export function formatDuration(start: string | null, end: string | null): string {
  if (!start) return '-';
  const secs = Math.round(durationMs(start, end) / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}
