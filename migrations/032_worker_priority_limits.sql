-- Worker priority + resource limits
-- Priority: higher = preferred for scheduling. 0 = default.
-- max_*: upper bound chola is allowed to reserve. NULL = use total_* (no limit).

ALTER TABLE workers ADD COLUMN IF NOT EXISTS priority INT DEFAULT 0;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_cpu INT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_memory_mb BIGINT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_disk_mb BIGINT;

-- Label group priority
ALTER TABLE label_groups ADD COLUMN IF NOT EXISTS priority INT DEFAULT 0;
