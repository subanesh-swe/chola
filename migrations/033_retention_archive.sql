-- Issue #20 — retention redesign — archive tables + stored functions
-- Adds files_purged_at to chola.job_groups, six *_archive tables mirroring
-- live shape with archived_at + no PK/FK, and chola.archive_group_ids /
-- chola.unarchive_group_ids stored functions for transactional moves with
-- correct FK ordering.

-- 1. Live-table column for T1 (files purged) bookkeeping.
ALTER TABLE chola.job_groups ADD COLUMN IF NOT EXISTS files_purged_at TIMESTAMPTZ;

-- 2. Archive tables. No primary key, no foreign keys, no UNIQUE, no CHECK.
--    Columns mirror the current live shape in the same order; archived_at
--    (and files_purged_at for job_groups_archive) is appended at the end.

CREATE TABLE IF NOT EXISTS chola.job_groups_archive (
    id                  UUID,
    repo_id             UUID,
    branch              VARCHAR(255),
    commit_sha          VARCHAR(64),
    trigger_source      VARCHAR(100),
    reserved_worker_id  VARCHAR(255),
    state               VARCHAR(20),
    created_at          TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    priority            INT,
    pr_number           INT,
    pinned              BOOLEAN,
    idempotency_key     VARCHAR(255),
    allocated_cpu       INT,
    allocated_memory_mb BIGINT,
    allocated_disk_mb   BIGINT,
    status_reason       TEXT,
    archived_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    files_purged_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_job_groups_archive_archived_at
    ON chola.job_groups_archive(archived_at);
CREATE INDEX IF NOT EXISTS idx_job_groups_archive_repo_id
    ON chola.job_groups_archive(repo_id);

CREATE TABLE IF NOT EXISTS chola.jobs_archive (
    id              UUID,
    job_group_id    UUID,
    stage_config_id UUID,
    stage_name      VARCHAR(255),
    command         TEXT,
    pre_script      TEXT,
    post_script     TEXT,
    worker_id       VARCHAR(255),
    state           VARCHAR(20),
    exit_code       INT,
    pre_exit_code   INT,
    post_exit_code  INT,
    log_path        TEXT,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ,
    retry_count     INT,
    status_reason   TEXT,
    archived_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_jobs_archive_archived_at
    ON chola.jobs_archive(archived_at);
CREATE INDEX IF NOT EXISTS idx_jobs_archive_group_id
    ON chola.jobs_archive(job_group_id);

CREATE TABLE IF NOT EXISTS chola.worker_reservations_archive (
    id             UUID,
    worker_id      VARCHAR(255),
    job_group_id   UUID,
    reserved_at    TIMESTAMPTZ,
    expires_at     TIMESTAMPTZ,
    released_at    TIMESTAMPTZ,
    release_reason VARCHAR(100),
    archived_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_worker_reservations_archive_archived_at
    ON chola.worker_reservations_archive(archived_at);
CREATE INDEX IF NOT EXISTS idx_worker_reservations_archive_group_id
    ON chola.worker_reservations_archive(job_group_id);

CREATE TABLE IF NOT EXISTS chola.artifacts_archive (
    id           UUID,
    job_group_id UUID,
    job_id       UUID,
    stage_name   VARCHAR(255),
    filename     VARCHAR(512),
    file_path    TEXT,
    size_bytes   BIGINT,
    content_type VARCHAR(255),
    created_at   TIMESTAMPTZ,
    archived_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_artifacts_archive_archived_at
    ON chola.artifacts_archive(archived_at);
CREATE INDEX IF NOT EXISTS idx_artifacts_archive_group_id
    ON chola.artifacts_archive(job_group_id);

CREATE TABLE IF NOT EXISTS chola.test_results_archive (
    id              UUID,
    job_id          UUID,
    job_group_id    UUID,
    suite_name      VARCHAR(512),
    test_name       VARCHAR(512),
    classname       VARCHAR(512),
    status          VARCHAR(20),
    duration_ms     INT,
    failure_message TEXT,
    failure_type    VARCHAR(255),
    stdout          TEXT,
    stderr          TEXT,
    created_at      TIMESTAMPTZ,
    archived_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_test_results_archive_archived_at
    ON chola.test_results_archive(archived_at);
CREATE INDEX IF NOT EXISTS idx_test_results_archive_group_id
    ON chola.test_results_archive(job_group_id);

CREATE TABLE IF NOT EXISTS chola.approval_gates_archive (
    id              UUID,
    job_group_id    UUID,
    stage_config_id UUID,
    status          VARCHAR(20),
    required_role   VARCHAR(50),
    requested_at    TIMESTAMPTZ,
    responded_at    TIMESTAMPTZ,
    responded_by    UUID,
    timeout_minutes INT,
    comment         TEXT,
    archived_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_approval_gates_archive_archived_at
    ON chola.approval_gates_archive(archived_at);
CREATE INDEX IF NOT EXISTS idx_approval_gates_archive_group_id
    ON chola.approval_gates_archive(job_group_id);

-- 3. Stored functions for transactional archive / unarchive.
--    Children copied first, parent last (so live FK pointing at job_groups
--    is satisfied throughout the INSERT pass). Parent deleted last (so FK
--    constraints on the live children don't fire mid-delete).

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
        allocated_disk_mb, status_reason, archived_at, files_purged_at
    )
    SELECT
        id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id,
        state, created_at, updated_at, completed_at, priority, pr_number,
        pinned, idempotency_key, allocated_cpu, allocated_memory_mb,
        allocated_disk_mb, status_reason, NOW(), files_purged_at
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
        allocated_disk_mb, status_reason, files_purged_at
    )
    SELECT
        id, repo_id, branch, commit_sha, trigger_source, reserved_worker_id,
        state, created_at, updated_at, completed_at, priority, pr_number,
        pinned, idempotency_key, allocated_cpu, allocated_memory_mb,
        allocated_disk_mb, status_reason, files_purged_at
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
