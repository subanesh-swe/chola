CREATE TABLE IF NOT EXISTS repos (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_name     VARCHAR(255) NOT NULL UNIQUE,
    repo_url      TEXT NOT NULL,
    default_branch VARCHAR(100) DEFAULT 'main',
    enabled       BOOLEAN DEFAULT true,
    created_at    TIMESTAMPTZ DEFAULT now(),
    updated_at    TIMESTAMPTZ DEFAULT now()
);

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

CREATE TABLE IF NOT EXISTS stage_scripts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stage_config_id UUID NOT NULL REFERENCES stage_configs(id),
    worker_id       VARCHAR(255),
    script_type     VARCHAR(10) NOT NULL CHECK (script_type IN ('pre', 'post')),
    script_scope    VARCHAR(10) NOT NULL CHECK (script_scope IN ('worker', 'master')),
    script          TEXT NOT NULL,
    created_at      TIMESTAMPTZ DEFAULT now(),
    updated_at      TIMESTAMPTZ DEFAULT now(),
    UNIQUE(stage_config_id, worker_id, script_type, script_scope)
);
