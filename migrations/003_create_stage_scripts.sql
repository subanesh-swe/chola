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
