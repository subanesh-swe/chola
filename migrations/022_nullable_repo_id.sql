-- Allow ad-hoc jobs submitted via gRPC submit_job without a repo
ALTER TABLE job_groups ALTER COLUMN repo_id DROP NOT NULL;
