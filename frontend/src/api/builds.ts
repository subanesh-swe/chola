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

export const listBuilds = (params?: ListBuildsParams) =>
  apiClient.get<PaginatedResponse<JobGroup>>('/job-groups', { params }).then((r) => r.data);

export const getBuild = (id: string) =>
  apiClient.get<{ job_group: JobGroup; jobs: Job[] }>(`/job-groups/${id}`).then((r) => r.data);

export const cancelBuild = (id: string, reason?: string) =>
  apiClient.post(`/job-groups/${id}/cancel`, { reason }).then((r) => r.data);

export const listBuildJobs = (groupId: string, params?: { limit?: number; offset?: number }) =>
  apiClient.get<PaginatedResponse<Job>>(`/job-groups/${groupId}/jobs`, { params }).then((r) => r.data);
