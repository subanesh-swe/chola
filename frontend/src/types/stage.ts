export interface Repo {
  id: string;
  repo_name: string;
  repo_url: string;
  default_branch: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface StageConfig {
  id: string;
  repo_id: string;
  stage_name: string;
  command: string;
  required_cpu: number;
  required_memory_mb: number;
  required_disk_mb: number;
  max_duration_secs: number;
  execution_order: number;
  parallel_group: string | null;
  allow_worker_migration: boolean;
  job_type: string;
  created_at: string;
  updated_at: string;
}

export interface StageScript {
  id: string;
  stage_config_id: string;
  worker_id: string | null;
  script_type: 'pre' | 'post';
  script_scope: 'worker' | 'master';
  script: string;
  created_at: string;
  updated_at: string;
}

export interface CreateRepoRequest {
  repo_name: string;
  repo_url: string;
  default_branch?: string;
}

export interface CreateStageConfigRequest {
  stage_name: string;
  command: string;
  required_cpu?: number;
  required_memory_mb?: number;
  required_disk_mb?: number;
  max_duration_secs?: number;
  execution_order?: number;
  parallel_group?: string;
  job_type?: string;
}
