export interface CommandBlacklistEntry {
  id: string;
  repo_id: string | null;
  stage_config_id: string | null;
  pattern: string;
  description: string | null;
  enabled: boolean;
  created_at: string;
}

export interface BranchBlacklistEntry {
  id: string;
  worker_id: string;
  pattern: string;
  description: string | null;
  enabled: boolean;
  created_at: string;
}
