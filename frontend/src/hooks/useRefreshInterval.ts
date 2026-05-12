import { useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

/**
 * URL-backed auto-refresh interval (in seconds). 0 = off.
 *
 * Stored under `?refresh=N` in the page URL so the choice is shareable,
 * survives copy/paste, and is captured in browser history.
 *
 * The `_key` argument is kept for API compatibility with callers but unused —
 * URL search params are global per page anyway.
 *
 * Per-page defaults: pass your own default (e.g. 5 on Builds, 30 on Analytics,
 * 0 on Runs). When the current value equals the default, `?refresh` is omitted
 * from the URL to keep it clean.
 */
export function useRefreshInterval(_key: string, defaultSecs = 0) {
  const [params, setParams] = useSearchParams();

  const raw = params.get('refresh');
  const secs = (() => {
    if (raw === null) return defaultSecs;
    const n = Number(raw);
    return Number.isFinite(n) && n >= 0 ? n : defaultSecs;
  })();

  const setSecs = useCallback(
    (s: number) => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (s === defaultSecs) {
            // Keep the URL clean when value matches the default.
            next.delete('refresh');
          } else {
            next.set('refresh', String(s));
          }
          return next;
        },
        { replace: true },
      );
    },
    [setParams, defaultSecs],
  );

  return [secs, setSecs] as const;
}
