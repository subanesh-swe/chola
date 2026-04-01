import apiClient from './client';
import type { Repo, StageConfig, CreateRepoRequest, CreateStageConfigRequest } from '../types';

interface PaginatedResponse<T> {
  data: T[];
  pagination: { total: number; limit: number; offset: number };
}

export const listRepos = (params?: { limit?: number; offset?: number }) =>
  apiClient.get<PaginatedResponse<Repo>>('/repos', { params }).then((r) => r.data);

export const getRepo = (id: string) =>
  apiClient.get<Repo>(`/repos/${id}`).then((r) => r.data);

export const createRepo = (data: CreateRepoRequest) =>
  apiClient.post<Repo>('/repos', data).then((r) => r.data);

export const updateRepo = (id: string, data: Partial<CreateRepoRequest>) =>
  apiClient.put<Repo>(`/repos/${id}`, data).then((r) => r.data);

export const deleteRepo = (id: string) =>
  apiClient.delete(`/repos/${id}`).then((r) => r.data);

export const listStageConfigs = (repoId: string) =>
  apiClient.get<{ stages: StageConfig[] }>(`/repos/${repoId}/stages`).then((r) => r.data);

export const createStageConfig = (repoId: string, data: CreateStageConfigRequest) =>
  apiClient.post<StageConfig>(`/repos/${repoId}/stages`, data).then((r) => r.data);

export const updateStageConfig = (repoId: string, stageId: string, data: Partial<CreateStageConfigRequest>) =>
  apiClient.put<StageConfig>(`/repos/${repoId}/stages/${stageId}`, data).then((r) => r.data);

export const deleteStageConfig = (repoId: string, stageId: string) =>
  apiClient.delete(`/repos/${repoId}/stages/${stageId}`).then((r) => r.data);

export const listWebhooks = (repoId: string) =>
  apiClient.get<{ webhooks: import('../types').Webhook[]; count: number }>(`/repos/${repoId}/webhooks`).then((r) => r.data);

export const createWebhook = (repoId: string, data: { provider: string; events?: string[] }) =>
  apiClient.post<import('../types').Webhook>(`/repos/${repoId}/webhooks`, data).then((r) => r.data);

export const deleteWebhook = (repoId: string, webhookId: string) =>
  apiClient.delete(`/repos/${repoId}/webhooks/${webhookId}`).then((r) => r.data);

export const listWebhookDeliveries = (repoId: string, webhookId: string) =>
  apiClient
    .get<{ deliveries: import('../types').WebhookDelivery[] }>(`/repos/${repoId}/webhooks/${webhookId}/deliveries`)
    .then((r) => r.data);

// ── Schedules ────────────────────────────────────────────────────────────────

export interface Schedule {
  id: string;
  repo_id: string;
  interval_secs: number;
  stages: string[];
  branch: string;
  enabled: boolean;
  next_run_at: string;
  last_triggered_at: string | null;
  created_at: string;
}

export const listSchedules = (repoId: string) =>
  apiClient.get<{ schedules: Schedule[] }>(`/repos/${repoId}/schedules`).then((r) => r.data);

export const createSchedule = (repoId: string, data: { interval_secs: number; stages: string[]; branch?: string }) =>
  apiClient.post<Schedule>(`/repos/${repoId}/schedules`, data).then((r) => r.data);

export const updateSchedule = (repoId: string, scheduleId: string, data: { interval_secs?: number; stages?: string[]; branch?: string; enabled?: boolean }) =>
  apiClient.put<Schedule>(`/repos/${repoId}/schedules/${scheduleId}`, data).then((r) => r.data);

export const deleteSchedule = (repoId: string, scheduleId: string) =>
  apiClient.delete(`/repos/${repoId}/schedules/${scheduleId}`).then((r) => r.data);
