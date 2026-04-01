-- command_mode: 'fixed' (default), 'optional', 'required'
-- fixed = only configured command runs, user cannot override
-- optional = configured command is default, user can override
-- required = user MUST provide command, no default
ALTER TABLE stage_configs ADD COLUMN IF NOT EXISTS command_mode VARCHAR(20) DEFAULT 'fixed';
ALTER TABLE stage_configs ALTER COLUMN command DROP NOT NULL;
