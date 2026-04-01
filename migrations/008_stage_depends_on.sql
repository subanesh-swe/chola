ALTER TABLE stage_configs ADD COLUMN IF NOT EXISTS depends_on TEXT[] DEFAULT '{}'
