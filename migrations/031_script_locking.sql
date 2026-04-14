-- Script locking: serialize concurrent script execution per lock key
-- Lock key is a template: {{WORKER_ID}}-{{REPO_NAME}}-pre resolves at runtime

-- Per-stage scripts
ALTER TABLE stage_scripts ADD COLUMN IF NOT EXISTS lock_enabled BOOLEAN DEFAULT FALSE;
ALTER TABLE stage_scripts ADD COLUMN IF NOT EXISTS lock_key TEXT;
ALTER TABLE stage_scripts ADD COLUMN IF NOT EXISTS lock_timeout_secs INT DEFAULT 120;

-- Global scripts (repo-level) — separate lock config for pre and post
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_pre_script_lock_enabled BOOLEAN DEFAULT FALSE;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_pre_script_lock_key TEXT;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_pre_script_lock_timeout_secs INT DEFAULT 120;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_post_script_lock_enabled BOOLEAN DEFAULT FALSE;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_post_script_lock_key TEXT;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_post_script_lock_timeout_secs INT DEFAULT 120;
