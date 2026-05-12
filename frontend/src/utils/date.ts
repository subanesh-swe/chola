/**
 * Returns a `YYYY-MM-DDTHH:mm` string (no seconds, no timezone suffix)
 * representing the current local time, suitable for `<input type="datetime-local">`.
 */
export function nowLocal(): string {
  const d = new Date();
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

/**
 * Returns a `YYYY-MM-DDTHH:mm` string for `hours` ago in local time,
 * suitable for `<input type="datetime-local">`.
 */
export function hoursAgoLocal(hours: number): string {
  const d = new Date(Date.now() - hours * 3600 * 1000);
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}
