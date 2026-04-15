import apiClient from './client';

export interface LabelGroup {
  id: string;
  name: string;
  match_labels: string[];
  env_vars: Record<string, string> | null;
  pre_script: string | null;
  max_concurrent_jobs: number;
  capabilities: string[];
  enabled: boolean;
  /** Scheduling priority for matched workers (0 = default) */
  priority: number;
  created_at: string;
  updated_at: string;
}

export interface LabelGroupRequest {
  name: string;
  match_labels: string[];
  env_vars?: Record<string, string>;
  pre_script?: string;
  max_concurrent_jobs?: number;
  capabilities?: string[];
  enabled?: boolean;
  priority?: number;
}

export interface UpdateLabelGroupRequest {
  name?: string;
  match_labels?: string[];
  env_vars?: Record<string, string>;
  pre_script?: string;
  max_concurrent_jobs?: number;
  capabilities?: string[];
  enabled?: boolean;
  priority?: number;
}

export const listLabelGroups = () =>
  apiClient.get<{ data: LabelGroup[] }>('/label-groups').then((r) => r.data);

export const getLabelGroup = (id: string) =>
  apiClient.get<LabelGroup>(`/label-groups/${id}`).then((r) => r.data);

export const createLabelGroup = (data: LabelGroupRequest) =>
  apiClient.post<LabelGroup>('/label-groups', data).then((r) => r.data);

export const updateLabelGroup = (id: string, data: UpdateLabelGroupRequest) =>
  apiClient.put<LabelGroup>(`/label-groups/${id}`, data).then((r) => r.data);

export const deleteLabelGroup = (id: string) =>
  apiClient.delete(`/label-groups/${id}`).then((r) => r.data);
