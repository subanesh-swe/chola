export type DiskType = 'nvme' | 'sata';
export type WorkerStatus = 'Connected' | 'Disconnected' | 'Draining';

export interface WorkerHeartbeat {
  used_cpu_percent: number;
  used_memory_mb: number;
  used_disk_mb: number;
  running_jobs: number;
  system_load: number;
  timestamp: string;
}

export interface Worker {
  worker_id: string;
  hostname: string;
  status: WorkerStatus;
  total_cpu: number;
  total_memory_mb: number;
  total_disk_mb: number;
  disk_type: string;
  docker_enabled: boolean;
  supported_job_types: string[];
  registered_at: string;
  last_heartbeat: WorkerHeartbeat | null;
}
