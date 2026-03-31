CREATE TABLE IF NOT EXISTS pipeline_variables (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id    UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name       VARCHAR(255) NOT NULL,
    value      TEXT NOT NULL,
    is_secret  BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(repo_id, name)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_variables_repo ON pipeline_variables(repo_id);
