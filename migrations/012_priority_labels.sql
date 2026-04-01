-- Task 23: build priority
ALTER TABLE job_groups ADD COLUMN IF NOT EXISTS priority INT DEFAULT 0;

-- Task 24: worker labels + stage required_labels
ALTER TABLE workers ADD COLUMN IF NOT EXISTS labels TEXT[] DEFAULT '{}';
ALTER TABLE stage_configs ADD COLUMN IF NOT EXISTS required_labels TEXT[] DEFAULT '{}';
