/**
 * Input: megabytes (API convention). Returns "512 MB", "2.0 GB", "1.5 TB".
 * precision default 1.
 */
export function formatBytes(mb: number, opts?: { precision?: number }): string {
  const precision = opts?.precision ?? 1;
  if (mb >= 1024 * 1024) {
    return `${(mb / (1024 * 1024)).toFixed(precision)} TB`;
  }
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(precision)} GB`;
  }
  return `${mb.toFixed(precision)} MB`;
}

/**
 * Input: raw bytes. Delegates to formatBytes after converting.
 */
export function formatBytesFromBytes(bytes: number): string {
  return formatBytes(bytes / (1024 * 1024));
}

/** Returns "0s" for 0, "5m"/"2h 30m" for positives, "—" for null/undefined. */
export function formatSecs(secs: number | null | undefined): string {
  if (secs == null) return '—';
  const total = Math.max(0, Math.floor(secs));
  if (total === 0) return '0s';
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  if (m > 0) return s > 0 ? `${m}m ${s}s` : `${m}m`;
  return `${s}s`;
}

/**
 * Adds thousand separators via Intl.NumberFormat.
 */
export function formatNumber(n: number): string {
  return new Intl.NumberFormat().format(n);
}
