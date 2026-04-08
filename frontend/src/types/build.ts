export type JobGroupState = 'pending' | 'reserved' | 'running' | 'success' | 'failed' | 'cancelled';

export interface AllocatedResources {
  cpu: number;
  memory_mb: number;
  disk_mb: number;
}

export interface JobGroup {
  id: string;
  job_group_id: string;
  repo_id: string;
  repo_name?: string;
  branch: string | null;
  commit_sha: string | null;
  trigger_source: string;
  reserved_worker_id: string | null;
  state: JobGroupState;
  allocated_resources?: AllocatedResources;
  last_activity_at?: string | null;
  time_until_timeout_secs?: number | null;
  idle_timeout_secs?: number | null;
  stall_timeout_secs?: number | null;
  status_reason?: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}
