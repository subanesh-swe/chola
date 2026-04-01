import apiClient from './client';
import type { CommandBlacklistEntry, BranchBlacklistEntry } from '../types';

export const listCommandBlacklist = (repoId?: string, stageConfigId?: string) =>
  apiClient
    .get<{ entries: CommandBlacklistEntry[] }>('/blacklist/commands', {
      params: { repo_id: repoId, stage_config_id: stageConfigId },
    })
    .then((r) => r.data);

export const createCommandBlacklist = (data: {
  repo_id?: string;
  stage_config_id?: string;
  pattern: string;
  description?: string;
}) => apiClient.post<CommandBlacklistEntry>('/blacklist/commands', data).then((r) => r.data);

export const updateCommandBlacklist = (
  id: string,
  data: { pattern?: string; description?: string; enabled?: boolean },
) => apiClient.put<CommandBlacklistEntry>(`/blacklist/commands/${id}`, data).then((r) => r.data);

export const deleteCommandBlacklist = (id: string) =>
  apiClient.delete(`/blacklist/commands/${id}`).then((r) => r.data);

export const listBranchBlacklist = (workerId?: string) =>
  apiClient
    .get<{ entries: BranchBlacklistEntry[] }>('/blacklist/branches', {
      params: { worker_id: workerId },
    })
    .then((r) => r.data);

export const createBranchBlacklist = (data: {
  worker_id: string;
  pattern: string;
  description?: string;
}) => apiClient.post<BranchBlacklistEntry>('/blacklist/branches', data).then((r) => r.data);

export const updateBranchBlacklist = (
  id: string,
  data: { pattern?: string; description?: string; enabled?: boolean },
) => apiClient.put<BranchBlacklistEntry>(`/blacklist/branches/${id}`, data).then((r) => r.data);

export const deleteBranchBlacklist = (id: string) =>
  apiClient.delete(`/blacklist/branches/${id}`).then((r) => r.data);
