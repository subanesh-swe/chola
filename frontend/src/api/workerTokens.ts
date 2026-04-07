import apiClient from './client';

export interface WorkerToken {
  id: string;
  name: string;
  scope: string;
  expires_at: string | null;
  max_uses: number;
  use_count: number;
  active: boolean;
  created_by: string | null;
  created_at: string;
}

export interface CreatedWorkerToken extends WorkerToken {
  token: string;
}

export interface CreateTokenRequest {
  name: string;
  scope?: string;
  expires_at?: string;
  max_uses?: number;
  worker_id?: string;
}

export const listWorkerTokens = () =>
  apiClient.get<{ data: WorkerToken[] }>('/worker-tokens').then((r) => r.data);

export const createWorkerToken = (data: CreateTokenRequest) =>
  apiClient.post<CreatedWorkerToken>('/worker-tokens', data).then((r) => r.data);

export const activateWorkerToken = (id: string) =>
  apiClient.put(`/worker-tokens/${id}/activate`).then((r) => r.data);

export const deactivateWorkerToken = (id: string) =>
  apiClient.put(`/worker-tokens/${id}/deactivate`).then((r) => r.data);

export const deleteWorkerToken = (id: string) =>
  apiClient.delete(`/worker-tokens/${id}`).then((r) => r.data);
