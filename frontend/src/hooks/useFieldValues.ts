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

async function fetchRepoValues(): Promise<FieldValue[]> {
  const resp = await listRepos({ limit: 200 });
  return (resp.data ?? []).map((r) => ({ value: r.id, label: r.repo_name }));
}

async function fetchStageValues(): Promise<FieldValue[]> {
  const reposResp = await listRepos({ limit: 200 });
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
}

async function fetchBranchValues(): Promise<FieldValue[]> {
  const branches = await listDistinctBranches();
  return branches.map((b) => ({ value: b }));
}

export function useFieldValues(): Record<string, FieldValue[] | (() => Promise<FieldValue[]>)> {
  return {
    state: STATE_VALUES,
    exit_code: EXIT_CODE_VALUES,
    bucket: BUCKET_VALUES,
    repo: fetchRepoValues,
    stage: fetchStageValues,
    branch: fetchBranchValues,
  };
}
