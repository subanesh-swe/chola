CREATE TABLE IF NOT EXISTS job_groups (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id            UUID NOT NULL REFERENCES repos(id),
    branch             VARCHAR(255),
    commit_sha         VARCHAR(64),
    trigger_source     VARCHAR(100) DEFAULT 'jenkins',
    reserved_worker_id VARCHAR(255),
    state              VARCHAR(20) DEFAULT 'pending'
                       CHECK (state IN ('pending','reserved','running','success','failed','cancelled')),
    created_at         TIMESTAMPTZ DEFAULT now(),
    updated_at         TIMESTAMPTZ DEFAULT now(),
    completed_at       TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_job_groups_state ON job_groups(state);
CREATE INDEX IF NOT EXISTS idx_job_groups_repo ON job_groups(repo_id);

CREATE TABLE IF NOT EXISTS jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_group_id    UUID NOT NULL REFERENCES job_groups(id),
    stage_config_id UUID NOT NULL REFERENCES stage_configs(id),
    stage_name      VARCHAR(255) NOT NULL,
    command         TEXT NOT NULL,
    pre_script      TEXT,
    post_script     TEXT,
    worker_id       VARCHAR(255),
    state           VARCHAR(20) DEFAULT 'queued'
                    CHECK (state IN ('queued','assigned','running','success','failed','cancelled','unknown')),
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
