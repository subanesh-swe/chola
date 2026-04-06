ALTER TABLE job_groups ADD COLUMN IF NOT EXISTS idempotency_key VARCHAR(255);
CREATE UNIQUE INDEX IF NOT EXISTS idx_job_groups_idempotency_key
    ON job_groups(idempotency_key) WHERE idempotency_key IS NOT NULL;
