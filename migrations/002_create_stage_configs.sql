CREATE TABLE IF NOT EXISTS stage_configs (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id                UUID NOT NULL REFERENCES repos(id),
    stage_name             VARCHAR(255) NOT NULL,
    command                TEXT NOT NULL,
    required_cpu           INT DEFAULT 1,
    required_memory_mb     INT DEFAULT 512,
    required_disk_mb       INT DEFAULT 1024,
    max_duration_secs      INT DEFAULT 3600,
    execution_order        INT DEFAULT 0,
    parallel_group         VARCHAR(100),
    allow_worker_migration BOOLEAN DEFAULT false,
    job_type               VARCHAR(50) DEFAULT 'common',
    created_at             TIMESTAMPTZ DEFAULT now(),
    updated_at             TIMESTAMPTZ DEFAULT now(),
    UNIQUE(repo_id, stage_name)
);
