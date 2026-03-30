export interface DashboardSummary {
  workers: {
    connected: number;
    disconnected: number;
    draining: number;
  };
  job_groups: {
    pending: number;
    running: number;
    success: number;
    failed: number;
  };
  recent_builds: Array<{
    job_group_id: string;
    repo_name?: string;
    branch: string | null;
    state: string;
    created_at: string;
  }>;
}
