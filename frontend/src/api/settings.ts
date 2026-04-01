import apiClient from './client';

export interface SystemSettings {
  auth: {
    enabled: boolean;
    jwt_expiry_secs: number;
  };
  scheduling: {
    strategy: string;
    nvme_preference: boolean;
    branch_affinity: boolean;
  };
  workers: {
    heartbeat_interval_secs: number;
    heartbeat_timeout_secs: number;
    max_reconnect_attempts: number;
    reservation_timeout_secs: number;
  };
  logging: {
    level: string;
    log_dir: string | null;
  };
}

export const getSettings = () =>
  apiClient.get<SystemSettings>('/settings').then((r) => r.data);
