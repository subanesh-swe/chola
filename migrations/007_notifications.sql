-- Notification configs per repo
CREATE TABLE IF NOT EXISTS notification_configs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id      UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    trigger      VARCHAR(20) NOT NULL CHECK (trigger IN ('on_success', 'on_failure', 'on_complete')),
    channel_type VARCHAR(20) NOT NULL CHECK (channel_type IN ('slack', 'webhook')),
    config       JSONB NOT NULL DEFAULT '{}',
    enabled      BOOLEAN NOT NULL DEFAULT true,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notification_configs_repo ON notification_configs(repo_id)
