import { useState, useEffect, useCallback, useMemo } from 'react';
import { useUrlFilters, type BuildFilters } from './useUrlFilters';

/**
 * Kibana-style draft/applied filter pair.
 *
 * - `applied` is what's in the URL and drives queries.
 * - `draft` is what the FilterBar inputs are bound to.
 * - User must call `apply()` (e.g. by clicking Search) to push draft → applied.
 * - If the URL changes externally (browser back/forward, reset), draft syncs to it.
 */
export function useAppliedFilters() {
  const { filters: applied, setFilters: setApplied, resetFilters } = useUrlFilters();
  const [draft, setDraft] = useState<BuildFilters>(applied);

  // Track the applied snapshot we last synced from. JSON.stringify is safe here:
  // BuildFilters only holds primitives + a string[]. Stable order is guaranteed
  // because parseFilters always populates fields in the same order.
  const appliedKey = useMemo(() => JSON.stringify(applied), [applied]);

  useEffect(() => {
    setDraft(applied);
    // appliedKey is the stable identity check; depending on `applied` directly
    // would re-fire on every render because it's a fresh object.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appliedKey]);

  const patchDraft = useCallback((patch: Partial<BuildFilters>) => {
    setDraft((d) => ({ ...d, ...patch }));
  }, []);

  const apply = useCallback(() => {
    setApplied(draft);
  }, [draft, setApplied]);

  /**
   * Apply a patch directly without going through draft state. Use for one-click
   * actions like pagination or preset buttons that should not require a Search press.
   */
  const applyPatch = useCallback(
    (patch: Partial<BuildFilters>) => {
      setDraft((d) => {
        const next = { ...d, ...patch };
        setApplied(next);
        return next;
      });
    },
    [setApplied],
  );

  const reset = useCallback(() => {
    resetFilters();
  }, [resetFilters]);

  const isDirty = JSON.stringify(draft) !== appliedKey;

  return { applied, draft, patchDraft, apply, applyPatch, reset, isDirty };
}
