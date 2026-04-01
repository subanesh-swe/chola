-- Add ON DELETE CASCADE to worker_reservations FK so retention cleanup can delete job_groups
ALTER TABLE worker_reservations
    DROP CONSTRAINT IF EXISTS worker_reservations_job_group_id_fkey;
ALTER TABLE worker_reservations
    ADD CONSTRAINT worker_reservations_job_group_id_fkey
        FOREIGN KEY (job_group_id) REFERENCES job_groups(id) ON DELETE CASCADE;
