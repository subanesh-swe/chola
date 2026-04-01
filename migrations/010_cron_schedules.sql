CREATE TABLE IF NOT EXISTS cron_schedules (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_id           UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    interval_secs     BIGINT NOT NULL,
    next_run_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    stages            TEXT[] NOT NULL DEFAULT '{}',
    branch            VARCHAR(255) NOT NULL DEFAULT 'main',
    enabled           BOOLEAN NOT NULL DEFAULT true,
    last_triggered_at TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cron_schedules_repo_id ON cron_schedules(repo_id);
CREATE INDEX IF NOT EXISTS idx_cron_schedules_next_run ON cron_schedules(next_run_at) WHERE enabled = true
