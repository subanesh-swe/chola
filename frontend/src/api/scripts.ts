import apiClient from './client';
import type { StageScript } from '../types';

interface ScriptsResponse {
  scripts: StageScript[];
  count: number;
}

export interface CreateScriptRequest {
  script_type: 'pre' | 'post';
  script_scope: 'worker' | 'master';
  script: string;
  worker_id?: string;
  lock_enabled?: boolean;
  lock_key?: string | null;
  lock_timeout_secs?: number;
}

export type UpdateScriptRequest = Partial<CreateScriptRequest>;

export const listScripts = (repoId: string, stageId: string) =>
  apiClient.get<ScriptsResponse>(`/repos/${repoId}/stages/${stageId}/scripts`).then((r) => r.data);

export const createScript = (repoId: string, stageId: string, data: CreateScriptRequest) =>
  apiClient.post<StageScript>(`/repos/${repoId}/stages/${stageId}/scripts`, data).then((r) => r.data);

export const updateScript = (repoId: string, stageId: string, scriptId: string, data: UpdateScriptRequest) =>
  apiClient.put<StageScript>(`/repos/${repoId}/stages/${stageId}/scripts/${scriptId}`, data).then((r) => r.data);

export const deleteScript = (repoId: string, stageId: string, scriptId: string) =>
  apiClient.delete(`/repos/${repoId}/stages/${stageId}/scripts/${scriptId}`).then((r) => r.data);
