-- 034_reserved_stages.sql
--
-- Track the granted (effective) stage list at reservation time so the
-- controller can wait for ALL reserved stages to complete before marking
-- a job group terminal. Today `check_group_completion` only inspects
-- submitted jobs, so a group of [a, b] gets marked Success the instant
-- stage `a` finishes — even if `b` is reserved but never submitted.
--
-- The new column stores the post-silent-filter stage list (i.e. only
-- stages that are actually configured for the repo). Pre-existing rows
-- get NULL → handled as "old shape, keep current behavior" by the
-- runtime; we don't backfill.
--
-- Mirror the column on the archive table so retention sweeps stay schema-
-- compatible with the live table.

BEGIN;

ALTER TABLE chola.job_groups
    ADD COLUMN IF NOT EXISTS reserved_stages TEXT[];

ALTER TABLE chola.job_groups_archive
    ADD COLUMN IF NOT EXISTS reserved_stages TEXT[];

COMMENT ON COLUMN chola.job_groups.reserved_stages IS
    'Granted stage names at reservation time (post silent-filter). When NULL or empty, group completion falls back to legacy "first-finished-stage wins" semantics.';

-- ─────────────────────────────────────────────────────────────────────
-- Update the archive / unarchive stored functions (migration 033) so
-- reserved_stages survives the live ↔ archive round-trip.
-- ─────────────────────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION chola.archive_group_ids(group_ids UUID[])
RETURNS BIGINT
LANGUAGE plpgsql
VOLATILE
AS $fn$
DECLARE
    affected BIGINT;
BEGIN
    -- INSERT children first, parent last (FK to job_groups still live).
    INSERT INTO chola.approval_gates_archive (
        id, job_group_id, stage_config_id, status, required_role,
        requested_at, responded_at, responded_by, timeout_minutes, comment,
        archived_at
    )
    SELECT
        id, job_group_id, stage_config_id, status, required_role,
        requested_at, responded_at, responded_by, timeout_minutes, comment,
        NOW()
    FROM chola.approval_gates
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.test_results_archive (
        id, job_id, job_group_id, suite_name, test_name, classname, status,
        duration_ms, failure_message, failure_type, stdout, stderr,
        created_at, archived_at
    )
    SELECT
        id, job_id, job_group_id, suite_name, test_name, classname, status,
        duration_ms, failure_message, failure_type, stdout, stderr,
        created_at, NOW()
    FROM chola.test_results
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.artifacts_archive (
        id, job_group_id, job_id, stage_name, filename, file_path,
        size_bytes, content_type, created_at, archived_at
    )
    SELECT
        id, job_group_id, job_id, stage_name, filename, file_path,
        size_bytes, content_type, created_at, NOW()
    FROM chola.artifacts
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.worker_reservations_archive (
        id, worker_id, job_group_id, reserved_at, expires_at, released_at,
        release_reason, archived_at
    )
    SELECT
        id, worker_id, job_group_id, reserved_at, expires_at, released_at,
        release_reason, NOW()
    FROM chola.worker_reservations
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.jobs_archive (
        id, job_group_id, stage_config_id, stage_name, command, pre_script,
        post_script, worker_id, state, exit_code, pre_exit_code,
        post_exit_code, log_path, started_at, completed_at, created_at,
        updated_at, retry_count, status_reason, archived_at
    )
    SELECT
        id, job_group_id, stage_config_id, stage_name, command, pre_script,
        post_script, worker_id, state, exit_code, pre_exit_code,
        post_exit_code, log_path, started_at, completed_at, created_at,
        updated_at, retry_count, status_reason, NOW()
    FROM chola.jobs
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.job_groups_archive (
        id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id,
        state, created_at, updated_at, completed_at, priority, pr_number,
        pinned, idempotency_key, allocated_cpu, allocated_memory_mb,
        allocated_disk_mb, reserved_stages, status_reason, archived_at,
        files_purged_at
    )
    SELECT
        id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id,
        state, created_at, updated_at, completed_at, priority, pr_number,
        pinned, idempotency_key, allocated_cpu, allocated_memory_mb,
        allocated_disk_mb, reserved_stages, status_reason, NOW(),
        files_purged_at
    FROM chola.job_groups
    WHERE id = ANY(group_ids);

    GET DIAGNOSTICS affected = ROW_COUNT;

    -- DELETE children first, parent last (FK from live children still
    -- references live parent until the parent row goes away).
    DELETE FROM chola.approval_gates      WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.test_results        WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.artifacts           WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.worker_reservations WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.jobs                WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.job_groups          WHERE id            = ANY(group_ids);

    RETURN affected;
END;
$fn$;

CREATE OR REPLACE FUNCTION chola.unarchive_group_ids(group_ids UUID[])
RETURNS BIGINT
LANGUAGE plpgsql
VOLATILE
AS $fn$
DECLARE
    affected BIGINT;
BEGIN
    -- Parent first into live so children's FK to job_groups is satisfied.
    INSERT INTO chola.job_groups (
        id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id,
        state, created_at, updated_at, completed_at, priority, pr_number,
        pinned, idempotency_key, allocated_cpu, allocated_memory_mb,
        allocated_disk_mb, reserved_stages, status_reason, files_purged_at
    )
    SELECT
        id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id,
        state, created_at, updated_at, completed_at, priority, pr_number,
        pinned, idempotency_key, allocated_cpu, allocated_memory_mb,
        allocated_disk_mb, reserved_stages, status_reason, files_purged_at
    FROM chola.job_groups_archive
    WHERE id = ANY(group_ids);

    GET DIAGNOSTICS affected = ROW_COUNT;

    INSERT INTO chola.jobs (
        id, job_group_id, stage_config_id, stage_name, command, pre_script,
        post_script, worker_id, state, exit_code, pre_exit_code,
        post_exit_code, log_path, started_at, completed_at, created_at,
        updated_at, retry_count, status_reason
    )
    SELECT
        id, job_group_id, stage_config_id, stage_name, command, pre_script,
        post_script, worker_id, state, exit_code, pre_exit_code,
        post_exit_code, log_path, started_at, completed_at, created_at,
        updated_at, retry_count, status_reason
    FROM chola.jobs_archive
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.worker_reservations (
        id, worker_id, job_group_id, reserved_at, expires_at, released_at,
        release_reason
    )
    SELECT
        id, worker_id, job_group_id, reserved_at, expires_at, released_at,
        release_reason
    FROM chola.worker_reservations_archive
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.artifacts (
        id, job_group_id, job_id, stage_name, filename, file_path,
        size_bytes, content_type, created_at
    )
    SELECT
        id, job_group_id, job_id, stage_name, filename, file_path,
        size_bytes, content_type, created_at
    FROM chola.artifacts_archive
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.test_results (
        id, job_id, job_group_id, suite_name, test_name, classname, status,
        duration_ms, failure_message, failure_type, stdout, stderr,
        created_at
    )
    SELECT
        id, job_id, job_group_id, suite_name, test_name, classname, status,
        duration_ms, failure_message, failure_type, stdout, stderr,
        created_at
    FROM chola.test_results_archive
    WHERE job_group_id = ANY(group_ids);

    INSERT INTO chola.approval_gates (
        id, job_group_id, stage_config_id, status, required_role,
        requested_at, responded_at, responded_by, timeout_minutes, comment
    )
    SELECT
        id, job_group_id, stage_config_id, status, required_role,
        requested_at, responded_at, responded_by, timeout_minutes, comment
    FROM chola.approval_gates_archive
    WHERE job_group_id = ANY(group_ids);

    -- Now strip the archive rows.
    DELETE FROM chola.approval_gates_archive      WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.test_results_archive        WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.artifacts_archive           WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.worker_reservations_archive WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.jobs_archive                WHERE job_group_id = ANY(group_ids);
    DELETE FROM chola.job_groups_archive          WHERE id            = ANY(group_ids);

    RETURN affected;
END;
$fn$;

COMMIT;
