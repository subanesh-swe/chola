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
}
