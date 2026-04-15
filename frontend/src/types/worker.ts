export type DiskType = 'nvme' | 'sata';
export type WorkerStatus = 'Connected' | 'Disconnected' | 'Draining';

export interface DiskDetail {
  mount_point: string;
  device: string;
  fs_type: string;
  total_mb: number;
  used_mb: number;
  available_mb: number;
}

export interface WorkerHeartbeat {
  used_cpu_percent: number;
  used_memory_mb: number;
  used_disk_mb: number;
  running_jobs: number;
  system_load: number;
  timestamp: string;
  disk_details?: DiskDetail[];
}

export interface WorkerSystemInfo {
  os_name?: string;
  os_version?: string;
  kernel_version?: string;
  arch?: string;
  host_name?: string;
  cpu_brand?: string;
  cpu_count?: number;
  boot_time?: number;
  uptime?: number;
}

export interface Worker {
  worker_id: string;
  hostname: string;
  status: WorkerStatus;
  total_cpu: number;
  allocated_cpu: number;
  available_cpu: number;
  total_memory_mb: number;
  allocated_memory_mb: number;
  available_memory_mb: number;
  total_disk_mb: number;
  allocated_disk_mb: number;
  available_disk_mb: number;
  disk_type: string;
  docker_enabled: boolean;
  supported_job_types: string[];
  registered_at: string;
  last_heartbeat: WorkerHeartbeat | null;
  disk_details?: DiskDetail[];
  system_info?: WorkerSystemInfo | null;
  active_groups?: WorkerActiveGroup[];
  labels?: string[];
  /** Scheduling priority — higher = preferred */
  priority: number;
  /** Hard cap on CPU cores (null = use total_cpu) */
  max_cpu: number | null;
  /** Hard cap on memory in MB (null = no limit) */
  max_memory_mb: number | null;
  /** Hard cap on disk in MB (null = no limit) */
  max_disk_mb: number | null;
  /** Percentage cap on CPU (1-100, null = no limit) */
  max_cpu_percent: number | null;
  /** Percentage cap on memory (1-100, null = no limit) */
  max_memory_percent: number | null;
  /** Percentage cap on disk (1-100, null = no limit) */
  max_disk_percent: number | null;
  /** Whether the worker has been approved (set via approve/reject API) */
  approved?: boolean;
  /** ID of the registration token used to register this worker */
  registration_token_id?: string | null;
}

export interface WorkerActiveGroup {
  group_id: string;
  state: string;
  branch: string | null;
  commit_sha: string | null;
  allocated_cpu: number;
  allocated_memory_mb: number;
  allocated_disk_mb: number;
  stages_submitted: number;
  created_at: string;
}
