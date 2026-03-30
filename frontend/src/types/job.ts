export type JobState = 'queued' | 'assigned' | 'running' | 'success' | 'failed' | 'cancelled' | 'unknown';
export type JobType = 'common' | 'heavy' | 'nix' | 'test';

export interface Job {
  id: string;
  job_group_id: string;
  stage_config_id: string;
  stage_name: string;
  command: string;
  pre_script: string | null;
  post_script: string | null;
  worker_id: string | null;
  state: JobState;
  exit_code: number | null;
  pre_exit_code: number | null;
  post_exit_code: number | null;
  log_path: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}
