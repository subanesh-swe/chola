CREATE TABLE IF NOT EXISTS job_groups (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id            UUID NOT NULL REFERENCES repos(id),
    branch             VARCHAR(255),
    commit_sha         VARCHAR(64),
    trigger_source     VARCHAR(100) DEFAULT 'jenkins',
    reserved_worker_id VARCHAR(255),
    state              VARCHAR(20) DEFAULT 'pending',
    created_at         TIMESTAMPTZ DEFAULT now(),
    updated_at         TIMESTAMPTZ DEFAULT now(),
    completed_at       TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_job_groups_state ON job_groups(state);
CREATE INDEX IF NOT EXISTS idx_job_groups_repo ON job_groups(repo_id);
