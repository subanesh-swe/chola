-- Partial index: only active jobs (replaces full idx_jobs_state_created)
CREATE INDEX IF NOT EXISTS idx_jobs_active
    ON jobs(state, created_at)
    WHERE state NOT IN ('success', 'failed', 'cancelled');

-- Partial index: active job groups
CREATE INDEX IF NOT EXISTS idx_job_groups_active
    ON job_groups(repo_id, branch, commit_sha)
    WHERE state NOT IN ('success', 'failed', 'cancelled');

-- BRIN for time-series queries on jobs
CREATE INDEX IF NOT EXISTS idx_jobs_created_brin
    ON jobs USING BRIN (created_at)
