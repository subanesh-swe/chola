import { listBuildsRaw } from './builds';

export async function listDistinctBranches(): Promise<string[]> {
  const resp = await listBuildsRaw({ limit: 200, offset: 0 });
  const set = new Set<string>();
  for (const b of resp.data ?? []) {
    if (b.branch && b.branch.trim()) set.add(b.branch.trim());
  }
  return Array.from(set).sort();
}
