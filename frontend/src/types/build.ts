export type JobGroupState = 'pending' | 'reserved' | 'running' | 'success' | 'failed' | 'cancelled';

export interface JobGroup {
  job_group_id: string;
  repo_id: string;
  repo_name?: string;
  branch: string | null;
  commit_sha: string | null;
  trigger_source: string;
  reserved_worker_id: string | null;
  state: JobGroupState;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}
