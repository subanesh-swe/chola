-- Add 'expired' to allowed job group states
ALTER TABLE job_groups DROP CONSTRAINT IF EXISTS job_groups_state_check;
ALTER TABLE job_groups ADD CONSTRAINT job_groups_state_check
    CHECK (state IN ('pending','reserved','running','success','failed','cancelled','expired'));

-- Persist allocated resources on job groups
ALTER TABLE job_groups ADD COLUMN IF NOT EXISTS allocated_cpu INT DEFAULT 0;
ALTER TABLE job_groups ADD COLUMN IF NOT EXISTS allocated_memory_mb BIGINT DEFAULT 0;
ALTER TABLE job_groups ADD COLUMN IF NOT EXISTS allocated_disk_mb BIGINT DEFAULT 0
