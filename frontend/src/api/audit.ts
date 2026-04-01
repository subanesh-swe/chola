import apiClient from './client';

export interface AuditEntry {
  id: string;
  user_id: string | null;
  username: string;
  action: string;
  resource_type: string | null;
  resource_id: string | null;
  details: Record<string, unknown> | null;
  ip_address: string | null;
  created_at: string;
}

export interface AuditLogResponse {
  entries: AuditEntry[];
  count: number;
}

export const listAuditLogs = (limit = 200) =>
  apiClient.get<AuditLogResponse>('/audit-log', { params: { limit } }).then((r) => r.data);
