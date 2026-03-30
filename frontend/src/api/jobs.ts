import apiClient from './client';
import type { Job } from '../types';

export const getJob = (id: string) =>
  apiClient.get<Job>(`/jobs/${id}`).then((r) => r.data);

export const cancelJob = (id: string, reason?: string) =>
  apiClient.post(`/jobs/${id}/cancel`, { reason }).then((r) => r.data);

export const getJobLogs = (id: string, offset?: number, limit?: number) =>
  apiClient.get<{ job_id: string; data: string; offset: number; total_bytes: number; complete: boolean }>(
    `/jobs/${id}/logs`, { params: { offset, limit } }
  ).then((r) => r.data);
