import apiClient from './client';

export interface Run {
  id: string;
  job_group_id: string;
  stage_name: string;
  command: string;
  worker_id: string | null;
  state: string;
  exit_code: number | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  branch: string | null;
  repo_name: string | null;
  group_state: string;
  trigger_source: string;
}

interface PaginatedResponse {
  data: Run[];
  pagination: { total: number; limit: number; offset: number };
}

export const listRuns = (params?: {
  limit?: number;
  offset?: number;
  state?: string;
  worker_id?: string;
}) =>
  apiClient
    .get<PaginatedResponse>('/runs', { params })
    .then((r) => r.data);
