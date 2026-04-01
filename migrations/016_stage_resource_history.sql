CREATE TABLE IF NOT EXISTS stage_resource_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stage_config_id UUID NOT NULL REFERENCES stage_configs(id) ON DELETE CASCADE,
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    job_id UUID NOT NULL,
    actual_cpu_percent FLOAT,
    actual_memory_mb BIGINT,
    actual_disk_mb BIGINT,
    actual_duration_secs INT,
    exit_code INT,
    created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_resource_history_stage ON stage_resource_history(stage_config_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_resource_history_repo ON stage_resource_history(repo_id);
