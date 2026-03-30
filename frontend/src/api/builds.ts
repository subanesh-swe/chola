import apiClient from './client';
import type { JobGroup, Job } from '../types';

interface ListBuildsParams {
  limit?: number;
  offset?: number;
  state?: string;
  repo_id?: string;
}

export const listBuilds = (params?: ListBuildsParams) =>
  apiClient.get<{ job_groups: JobGroup[]; total: number }>('/job-groups', { params }).then((r) => r.data);

export const getBuild = (id: string) =>
  apiClient.get<{ job_group: JobGroup; jobs: Job[] }>(`/job-groups/${id}`).then((r) => r.data);

export const cancelBuild = (id: string, reason?: string) =>
  apiClient.post(`/job-groups/${id}/cancel`, { reason }).then((r) => r.data);

export const listBuildJobs = (groupId: string) =>
  apiClient.get<{ jobs: Job[] }>(`/job-groups/${groupId}/jobs`).then((r) => r.data);
