-- Additional indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_job_groups_repo_state_created
    ON job_groups(repo_id, state, created_at);

CREATE INDEX IF NOT EXISTS idx_jobs_state_created
    ON jobs(state, created_at);

CREATE INDEX IF NOT EXISTS idx_jobs_group_state
    ON jobs(job_group_id, state);

CREATE INDEX IF NOT EXISTS idx_stage_configs_repo_order
    ON stage_configs(repo_id, execution_order);

-- Fix FK cascades: stage_configs -> repos
ALTER TABLE stage_configs
    DROP CONSTRAINT IF EXISTS stage_configs_repo_id_fkey,
    ADD CONSTRAINT stage_configs_repo_id_fkey
        FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE;

-- Fix FK cascades: stage_scripts -> stage_configs
ALTER TABLE stage_scripts
    DROP CONSTRAINT IF EXISTS stage_scripts_stage_config_id_fkey,
    ADD CONSTRAINT stage_scripts_stage_config_id_fkey
        FOREIGN KEY (stage_config_id) REFERENCES stage_configs(id) ON DELETE CASCADE;

-- Fix FK cascades: jobs -> job_groups
ALTER TABLE jobs
    DROP CONSTRAINT IF EXISTS jobs_job_group_id_fkey,
    ADD CONSTRAINT jobs_job_group_id_fkey
        FOREIGN KEY (job_group_id) REFERENCES job_groups(id) ON DELETE CASCADE;
