import { useMemo } from 'react';
import { listRepos, getStageNames } from '../api/repos';
import { listDistinctBranches } from '../api/branches';

export interface FieldValue {
  value: string;
  label?: string;
  hint?: string;
}

const STATE_VALUES: FieldValue[] = [
  { value: 'queued' },
  { value: 'reserved' },
  { value: 'running' },
  { value: 'success' },
  { value: 'failed' },
  { value: 'cancelled' },
  { value: 'expired' },
];

const EXIT_CODE_VALUES: FieldValue[] = [
  { value: '0', label: '0', hint: 'success' },
  { value: '!=0', label: '!=0', hint: 'any failure' },
  { value: '>=1', label: '>=1', hint: 'failure' },
];

const BUCKET_VALUES: FieldValue[] = [
  { value: 'auto', hint: 'auto-select granularity' },
  { value: 'hour', hint: 'hourly buckets' },
  { value: 'day', hint: 'daily buckets' },
];

const TTL_MS = 5 * 60 * 1000;

interface Cache {
  ts: number;
  promise: Promise<FieldValue[]> | null;
}

const repoCache: Cache = { ts: 0, promise: null };
const stageCache: Cache = { ts: 0, promise: null };
const branchCache: Cache = { ts: 0, promise: null };

function isFresh(cache: Cache): boolean {
  return cache.promise !== null && Date.now() - cache.ts < TTL_MS;
}

function fetchRepoValues(): Promise<FieldValue[]> {
  if (!isFresh(repoCache)) {
    repoCache.ts = Date.now();
    repoCache.promise = listRepos({ limit: 200 }).then((resp) =>
      (resp.data ?? []).map((r) => ({ value: r.id, label: r.repo_name })),
    );
  }
  return repoCache.promise!;
}

function fetchStageValues(): Promise<FieldValue[]> {
  if (!isFresh(stageCache)) {
    stageCache.ts = Date.now();
    stageCache.promise = listRepos({ limit: 200 }).then(async (reposResp) => {
      const repos = reposResp.data ?? [];
      const results = await Promise.all(repos.map((r) => getStageNames(r.id).catch(() => [])));
      const seen = new Set<string>();
      const out: FieldValue[] = [];
      for (const names of results) {
        for (const name of names) {
          if (!seen.has(name)) {
            seen.add(name);
            out.push({ value: name });
          }
        }
      }
      return out.sort((a, b) => a.value.localeCompare(b.value));
    });
  }
  return stageCache.promise!;
}

function fetchBranchValues(): Promise<FieldValue[]> {
  if (!isFresh(branchCache)) {
    branchCache.ts = Date.now();
    branchCache.promise = listDistinctBranches().then((branches) =>
      branches.map((b) => ({ value: b })),
    );
  }
  return branchCache.promise!;
}

const FIELD_VALUES_SINGLETON: Record<string, FieldValue[] | (() => Promise<FieldValue[]>)> = {
  state: STATE_VALUES,
  exit_code: EXIT_CODE_VALUES,
  bucket: BUCKET_VALUES,
  repo: fetchRepoValues,
  stage: fetchStageValues,
  branch: fetchBranchValues,
};

export function useFieldValues(): Record<string, FieldValue[] | (() => Promise<FieldValue[]>)> {
  return useMemo(() => FIELD_VALUES_SINGLETON, []);
}
