import apiClient from './client';
import type { Repo, StageConfig, CreateRepoRequest, CreateStageConfigRequest } from '../types';

export const listRepos = () =>
  apiClient.get<{ repos: Repo[]; count: number }>('/repos').then((r) => r.data);

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
