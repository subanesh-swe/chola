-- Global pre/post scripts: reservation-level scripts that run before the first
-- stage and after the last stage of a build pipeline.
-- Scope: 'worker' (runs on reserved worker), 'master' (runs on controller), or 'both'.

ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_pre_script TEXT;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_pre_script_scope VARCHAR(10) DEFAULT 'worker';
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_post_script TEXT;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS global_post_script_scope VARCHAR(10) DEFAULT 'worker';
