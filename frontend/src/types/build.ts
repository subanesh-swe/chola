export type JobGroupState = 'pending' | 'reserved' | 'running' | 'success' | 'failed' | 'cancelled';

export interface AllocatedResources {
  cpu: number;
  memory_mb: number;
  disk_mb: number;
}

/** Archived child records returned by GET /api/v1/job-groups/:id for archived groups. */
export interface ArchivedChildren {
  artifacts: unknown[];
  test_results: unknown[];
  approval_gates: unknown[];
  worker_reservations: unknown[];
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
  /** Granted stage manifest at reservation time (post silent-filter). */
  reserved_stages?: string[];
  /** Stage names that have a submitted job. Derived from `jobs[].stage_name`. */
  submitted_stages?: string[];
  /** Repo-level global scripts + scope (worker|controller|both). */
  global_pre_script?: string | null;
  global_pre_script_scope?: string | null;
  global_post_script?: string | null;
  global_post_script_scope?: string | null;
  allocated_resources?: AllocatedResources;
  last_activity_at?: string | null;
  time_until_timeout_secs?: number | null;
  idle_timeout_secs?: number | null;
  stall_timeout_secs?: number | null;
  status_reason?: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  /** Present in list responses when include_archived=true. */
  archived?: boolean;
  /** RFC3339 — set when the group has been moved to the archive table (T2). */
  archived_at?: string | null;
  /** RFC3339 — set when on-disk logs/workspace were purged by T1. */
  files_purged_at?: string | null;
  /** Only populated for archived groups in the detail endpoint. */
  children?: ArchivedChildren | null;
}
