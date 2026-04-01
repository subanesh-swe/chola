export interface BuildTrendPoint {
  date: string;
  total: number;
  success: number;
  failed: number;
}

export interface DurationTrendPoint {
  date: string;
  avg_duration_secs: number;
  p95_duration_secs: number;
}

export interface SlowStage {
  stage_name: string;
  repo_name: string;
  avg_secs: number;
}

export interface FailingRepo {
  repo_name: string;
  total: number;
  failed: number;
}

export interface WorkerUtilization {
  worker_id: string;
  hostname: string | null;
  status: string;
  active_jobs: number;
  total_jobs_30d: number;
}

export interface QueueWaitPoint {
  date: string;
  avg_wait_secs: number;
}

export interface AnalyticsSummary {
  total_builds: number;
  success_rate: number;
  avg_duration_secs: number;
  avg_queue_wait_secs: number;
}

export interface AnalyticsData {
  summary: AnalyticsSummary;
  build_trends: BuildTrendPoint[];
  duration_trends: DurationTrendPoint[];
  slowest_stages: SlowStage[];
  failing_repos: FailingRepo[];
  worker_utilization: WorkerUtilization[];
  queue_wait_trends: QueueWaitPoint[];
}
