-- Allow jobs created via gRPC submit_stage without a pre-configured stage_config
ALTER TABLE jobs ALTER COLUMN stage_config_id DROP NOT NULL;
