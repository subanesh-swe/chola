CREATE TABLE IF NOT EXISTS worker_reservations (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    worker_id      VARCHAR(255) NOT NULL,
    job_group_id   UUID NOT NULL REFERENCES job_groups(id),
    reserved_at    TIMESTAMPTZ DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL,
    released_at    TIMESTAMPTZ,
    release_reason VARCHAR(100),
    UNIQUE(worker_id, job_group_id)
);
CREATE INDEX IF NOT EXISTS idx_reservations_worker ON worker_reservations(worker_id);
CREATE INDEX IF NOT EXISTS idx_reservations_active ON worker_reservations(released_at) WHERE released_at IS NULL;
