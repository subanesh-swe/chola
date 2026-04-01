-- Stage-level blacklisted commands (regex patterns that are forbidden)
CREATE TABLE IF NOT EXISTS stage_command_blacklist (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id     UUID REFERENCES repos(id) ON DELETE CASCADE,
    stage_config_id UUID REFERENCES stage_configs(id) ON DELETE CASCADE,
    pattern     TEXT NOT NULL,
    description TEXT,
    enabled     BOOLEAN DEFAULT true,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_cmd_blacklist_repo ON stage_command_blacklist(repo_id);
CREATE INDEX IF NOT EXISTS idx_cmd_blacklist_stage ON stage_command_blacklist(stage_config_id);

-- Worker-level branch blacklist (branches that should NOT run on specific workers)
CREATE TABLE IF NOT EXISTS worker_branch_blacklist (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    worker_id   VARCHAR(255) NOT NULL,
    pattern     TEXT NOT NULL,
    description TEXT,
    enabled     BOOLEAN DEFAULT true,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_branch_blacklist_worker ON worker_branch_blacklist(worker_id);
