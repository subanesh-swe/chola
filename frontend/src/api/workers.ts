import apiClient from './client';
import type { Worker } from '../types';

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
