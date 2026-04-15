-- Worker priority + resource limits
-- Priority: higher = preferred for scheduling. 0 = default.
-- max_*: absolute upper bound. NULL = no limit (use total).
-- max_*_percent: percentage upper bound (1-100). NULL = no limit.
-- If both set, effective cap = min(absolute, total * percent / 100).

ALTER TABLE workers ADD COLUMN IF NOT EXISTS priority INT DEFAULT 0;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_cpu INT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_memory_mb BIGINT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_disk_mb BIGINT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_cpu_percent INT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_memory_percent INT;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS max_disk_percent INT;

-- Label group priority
ALTER TABLE label_groups ADD COLUMN IF NOT EXISTS priority INT DEFAULT 0;
