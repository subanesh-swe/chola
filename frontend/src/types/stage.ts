export interface Repo {
  id: string;
  repo_name: string;
  repo_url: string;
  default_branch: string;
  enabled: boolean;
  global_pre_script: string | null;
  global_pre_script_scope: string;
  global_pre_script_lock_enabled: boolean;
  global_pre_script_lock_key: string | null;
  global_pre_script_lock_timeout_secs: number;
  global_post_script: string | null;
  global_post_script_scope: string;
  global_post_script_lock_enabled: boolean;
  global_post_script_lock_key: string | null;
  global_post_script_lock_timeout_secs: number;
  created_at: string;
  updated_at: string;
}

export interface StageConfig {
  id: string;
  repo_id: string;
  stage_name: string;
  command: string | null;
  command_mode: string;
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
  lock_enabled: boolean;
  lock_key: string | null;
  lock_timeout_secs: number;
  created_at: string;
  updated_at: string;
}

export interface CreateRepoRequest {
  repo_name: string;
  repo_url: string;
  default_branch?: string;
  enabled?: boolean;
  global_pre_script?: string | null;
  global_pre_script_scope?: string;
  global_pre_script_lock_enabled?: boolean;
  global_pre_script_lock_key?: string | null;
  global_pre_script_lock_timeout_secs?: number;
  global_post_script?: string | null;
  global_post_script_scope?: string;
  global_post_script_lock_enabled?: boolean;
  global_post_script_lock_key?: string | null;
  global_post_script_lock_timeout_secs?: number;
}

export interface Webhook {
  id: string;
  repo_id: string;
  provider: string;
  secret: string;
  events: string[];
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface WebhookDelivery {
  id: string;
  webhook_id: string;
  event: string;
  status_code: number;
  response_time_ms: number;
  delivered_at: string;
  success: boolean;
}

export interface CreateStageConfigRequest {
  stage_name: string;
  command?: string | null;
  command_mode?: string;
  required_cpu?: number;
  required_memory_mb?: number;
  required_disk_mb?: number;
  max_duration_secs?: number;
  execution_order?: number;
  parallel_group?: string;
  job_type?: string;
}
