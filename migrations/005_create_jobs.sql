CREATE TABLE IF NOT EXISTS jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_group_id    UUID NOT NULL REFERENCES job_groups(id),
    stage_config_id UUID NOT NULL REFERENCES stage_configs(id),
    stage_name      VARCHAR(255) NOT NULL,
    command         TEXT NOT NULL,
    pre_script      TEXT,
    post_script     TEXT,
    worker_id       VARCHAR(255),
    state           VARCHAR(20) DEFAULT 'queued',
    exit_code       INT,
    pre_exit_code   INT,
    post_exit_code  INT,
    log_path        TEXT,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT now(),
    updated_at      TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_jobs_group ON jobs(job_group_id);
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
CREATE INDEX IF NOT EXISTS idx_jobs_worker ON jobs(worker_id);
