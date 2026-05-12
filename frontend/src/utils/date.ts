/**
 * Returns a `YYYY-MM-DDTHH:mm` string (no seconds, no timezone suffix) suitable
 * for `<input type="datetime-local">`. The value represents the current local time.
 */
export function nowIso(): string {
  const d = new Date();
  // Shift to local time by subtracting the timezone offset.
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

/**
 * Returns a `YYYY-MM-DDTHH:mm` string for `n` days before now (local time),
 * at the start of that day (00:00).
 */
export function subDaysIso(n: number): string {
  const d = new Date();
  d.setDate(d.getDate() - n);
  d.setHours(0, 0, 0, 0);
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}
