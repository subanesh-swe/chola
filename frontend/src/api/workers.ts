import apiClient from './client';
import type { Worker } from '../types';

export const listWorkers = () =>
  apiClient.get<{ workers: Worker[]; count: number }>('/workers').then((r) => r.data);

export const getWorker = (id: string) =>
  apiClient.get<Worker>(`/workers/${id}`).then((r) => r.data);

export const drainWorker = (id: string) =>
  apiClient.post(`/workers/${id}/drain`).then((r) => r.data);

export const undrainWorker = (id: string) =>
  apiClient.post(`/workers/${id}/undrain`).then((r) => r.data);
