ALTER TABLE worker_tokens ADD COLUMN IF NOT EXISTS worker_id VARCHAR(255);
CREATE UNIQUE INDEX IF NOT EXISTS idx_worker_tokens_worker_id
    ON worker_tokens(worker_id) WHERE worker_id IS NOT NULL;
