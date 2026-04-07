import apiClient from './client';
import type { Worker } from '../types';

export interface RegisterWorkerRequest {
  worker_id: string;
  hostname: string;
  labels?: string[];
  description?: string;
}

export interface RegisterWorkerResponse {
  worker_id: string;
  token: string;
}

export const registerWorker = (data: RegisterWorkerRequest) =>
  apiClient.post<RegisterWorkerResponse>('/workers/register', data).then((r) => r.data);

interface PaginatedResponse<T> {
  data: T[];
  pagination: { total: number; limit: number; offset: number };
}

export const listWorkers = (params?: { limit?: number; offset?: number }) =>
  apiClient.get<PaginatedResponse<Worker>>('/workers', { params }).then((r) => r.data);

export const getWorker = (id: string) =>
  apiClient.get<Worker>(`/workers/${id}`).then((r) => r.data);

export const drainWorker = (id: string) =>
  apiClient.post(`/workers/${id}/drain`).then((r) => r.data);

export const undrainWorker = (id: string) =>
  apiClient.post(`/workers/${id}/undrain`).then((r) => r.data);

export const approveWorker = (id: string) =>
  apiClient.put(`/workers/${id}/approve`).then((r) => r.data);

export const rejectWorker = (id: string) =>
  apiClient.put(`/workers/${id}/reject`).then((r) => r.data);

export interface RegenerateTokenResponse {
  worker_id: string;
  token: string;
}

export const regenerateWorkerToken = (workerId: string) =>
  apiClient
    .post<RegenerateTokenResponse>(`/workers/${workerId}/regenerate-token`)
    .then((r) => r.data);
