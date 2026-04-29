import apiClient from './client';
import type { JobGroup, Job } from '../types';
import type { BuildFilters } from '../hooks/useUrlFilters';

interface PaginatedResponse<T> {
  data: T[];
  pagination: { total: number; limit: number; offset: number };
}

export interface ListBuildsQueryParams {
  limit?: number;
  offset?: number;
  state?: string;
  repo_id?: string;
  branch?: string;
  date_from?: string;
  date_to?: string;
  stage_name?: string;
  exit_code?: number;
  sort_by?: string;
  sort_dir?: string;
}

// Normalize API response: backend returns `id`, frontend expects `job_group_id`
function normalizeGroup(g: JobGroup): JobGroup {
  return { ...g, job_group_id: g.job_group_id || g.id };
}

function filtersToParams(filters: Partial<BuildFilters>, limit: number, offset: number): ListBuildsQueryParams {
  const params: ListBuildsQueryParams = { limit, offset };

  if (filters.state?.length) params.state = filters.state.join(',');
  if (filters.repo) params.repo_id = filters.repo;
  if (filters.branch) params.branch = filters.branch;
  if (filters.dateFrom) params.date_from = filters.dateFrom;
  if (filters.dateTo) params.date_to = filters.dateTo;
  if (filters.stage) params.stage_name = filters.stage;
  if (filters.exitCode) {
    params.exit_code = filters.exitCode === 'nonzero' ? -1 : Number(filters.exitCode);
  }
  if (filters.sortKey) params.sort_by = filters.sortKey;
  if (filters.sortDir) params.sort_dir = filters.sortDir;

  return params;
}

const PAGE_SIZE = 20;

function fetchJobGroups(params: ListBuildsQueryParams) {
  return apiClient
    .get<PaginatedResponse<JobGroup>>('/job-groups', { params })
    .then((r) => ({ ...r.data, data: r.data.data.map(normalizeGroup) }));
}

export const listBuilds = (filters: Partial<BuildFilters> = {}) => {
  const page = filters.page ?? 1;
  const offset = (page - 1) * PAGE_SIZE;
  return fetchJobGroups(filtersToParams(filters, PAGE_SIZE, offset));
};

export const listBuildsRaw = (params: ListBuildsQueryParams) => fetchJobGroups(params);

export const getBuild = (id: string) =>
  apiClient.get<JobGroup & { jobs: Job[] }>(`/job-groups/${id}`).then((r) => {
    const { jobs, ...group } = r.data;
    return { job_group: normalizeGroup(group as JobGroup), jobs: jobs || [] };
  });

export const cancelBuild = (id: string, reason?: string) =>
  apiClient.post(`/job-groups/${id}/cancel`, { reason }).then((r) => r.data);

export const listBuildJobs = (groupId: string, params?: { limit?: number; offset?: number }) =>
  apiClient.get<PaginatedResponse<Job>>(`/job-groups/${groupId}/jobs`, { params }).then((r) => r.data);

export const retryBuild = (id: string) =>
  apiClient.post(`/job-groups/${id}/retry`).then((r) => r.data);

export const retryJob = (id: string) =>
  apiClient.post(`/jobs/${id}/retry`).then((r) => r.data);
