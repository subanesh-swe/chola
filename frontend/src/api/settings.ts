import apiClient from './client';

export interface SettingItem {
  key: string;
  value: string | number | boolean;
  source: 'database' | 'config' | 'default';
  editable: boolean;
  type?: 'bool' | 'int';
  options?: string[];
  min?: number;
  max?: number;
}

export interface SettingsResponse {
  settings: SettingItem[];
}

export const getSettings = () =>
  apiClient.get<SettingsResponse>('/settings').then((r) => r.data);

export const updateSetting = (key: string, value: string) =>
  apiClient.put('/settings', { key, value }).then((r) => r.data);

export const revertSetting = (key: string) =>
  apiClient.delete(`/settings/${encodeURIComponent(key)}`).then((r) => r.data);
