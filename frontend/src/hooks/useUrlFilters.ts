import { useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

export type SortDir = 'asc' | 'desc';

export interface BuildFilters {
  state: string[];
  repo: string;
  branch: string;
  dateFrom: string;
  dateTo: string;
  stage: string;
  exitCode: string;
  page: number;
  sortKey: string;
  sortDir: SortDir;
}

const DEFAULTS: BuildFilters = {
  state: [],
  repo: '',
  branch: '',
  dateFrom: '',
  dateTo: '',
  stage: '',
  exitCode: '',
  page: 1,
  sortKey: '',
  sortDir: 'desc',
};

function parseStates(raw: string | null): string[] {
  if (!raw) return [];
  return raw.split(',').filter(Boolean);
}

function parseFilters(p: URLSearchParams): BuildFilters {
  return {
    state: parseStates(p.get('state')),
    repo: p.get('repo') ?? '',
    branch: p.get('branch') ?? '',
    dateFrom: p.get('dateFrom') ?? '',
    dateTo: p.get('dateTo') ?? '',
    stage: p.get('stage') ?? '',
    exitCode: p.get('exitCode') ?? '',
    page: Number(p.get('page') ?? '1') || 1,
    sortKey: p.get('sortKey') ?? '',
    sortDir: (p.get('sortDir') as SortDir) ?? 'desc',
  };
}

export function useUrlFilters() {
  const [params, setParams] = useSearchParams();

  const filters: BuildFilters = parseFilters(params);

  const setFilters = useCallback(
    (patch: Partial<BuildFilters>) => {
      setParams((prev) => {
        const next = new URLSearchParams(prev);
        const merged = { ...parseFilters(prev), ...patch };

        if (merged.state.length) next.set('state', merged.state.join(','));
        else next.delete('state');

        if (merged.repo) next.set('repo', merged.repo);
        else next.delete('repo');

        if (merged.branch) next.set('branch', merged.branch);
        else next.delete('branch');

        if (merged.dateFrom) next.set('dateFrom', merged.dateFrom);
        else next.delete('dateFrom');

        if (merged.dateTo) next.set('dateTo', merged.dateTo);
        else next.delete('dateTo');

        if (merged.stage) next.set('stage', merged.stage);
        else next.delete('stage');

        if (merged.exitCode) next.set('exitCode', merged.exitCode);
        else next.delete('exitCode');

        if (merged.sortKey) next.set('sortKey', merged.sortKey);
        else next.delete('sortKey');

        next.set('sortDir', merged.sortDir);

        const pageVal = 'page' in patch ? patch.page! : 1;
        if (pageVal > 1) next.set('page', String(pageVal));
        else next.delete('page');

        return next;
      });
    },
    [setParams],
  );

  const resetFilters = useCallback(() => {
    setParams(new URLSearchParams());
  }, [setParams]);

  return { filters, setFilters, resetFilters, defaults: DEFAULTS };
}
