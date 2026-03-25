CREATE TABLE IF NOT EXISTS workers (
    worker_id          VARCHAR(255) PRIMARY KEY,
    hostname           VARCHAR(255),
    total_cpu          INT,
    total_memory_mb    INT,
    total_disk_mb      INT,
    disk_type          VARCHAR(10),
    supported_job_types TEXT[],
    docker_enabled     BOOLEAN DEFAULT false,
    status             VARCHAR(20) DEFAULT 'disconnected',
    last_heartbeat_at  TIMESTAMPTZ,
    registered_at      TIMESTAMPTZ DEFAULT now()
);
