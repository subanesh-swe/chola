import apiClient from './client';
import type { JobGroup, Job } from '../types';

interface ListBuildsParams {
  limit?: number;
  offset?: number;
  state?: string;
  repo_id?: string;
  branch?: string;
  date_from?: string;
  date_to?: string;
  sort_by?: string;
  sort_dir?: string;
}

interface PaginatedResponse<T> {
  data: T[];
  pagination: { total: number; limit: number; offset: number };
}

// Normalize API response: backend returns `id`, frontend expects `job_group_id`
function normalizeGroup(g: JobGroup): JobGroup {
  return { ...g, job_group_id: g.job_group_id || g.id };
}

export const listBuilds = (params?: ListBuildsParams) =>
  apiClient.get<PaginatedResponse<JobGroup>>('/job-groups', { params }).then((r) => ({
    ...r.data,
    data: r.data.data.map(normalizeGroup),
  }));

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
