-- Webhooks table for GitHub/GitLab push/PR triggers
CREATE TABLE IF NOT EXISTS webhooks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id     UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    provider    VARCHAR(20) NOT NULL CHECK (provider IN ('github', 'gitlab')),
    secret      VARCHAR(255) NOT NULL UNIQUE,
    events      TEXT[] NOT NULL DEFAULT '{"push"}',
    enabled     BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ DEFAULT now(),
    updated_at  TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_webhooks_repo ON webhooks(repo_id);
CREATE INDEX IF NOT EXISTS idx_webhooks_secret ON webhooks(secret)
