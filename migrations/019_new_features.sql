-- Build artifacts metadata
CREATE TABLE IF NOT EXISTS artifacts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_group_id    UUID NOT NULL REFERENCES job_groups(id) ON DELETE CASCADE,
    job_id          UUID REFERENCES jobs(id) ON DELETE CASCADE,
    stage_name      VARCHAR(255) NOT NULL,
    filename        VARCHAR(512) NOT NULL,
    file_path       TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL DEFAULT 0,
    content_type    VARCHAR(255) DEFAULT 'application/octet-stream',
    created_at      TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_artifacts_group ON artifacts(job_group_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_job ON artifacts(job_id);

-- JUnit test results
CREATE TABLE IF NOT EXISTS test_results (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id          UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    job_group_id    UUID NOT NULL REFERENCES job_groups(id) ON DELETE CASCADE,
    suite_name      VARCHAR(512) NOT NULL,
    test_name       VARCHAR(512) NOT NULL,
    classname       VARCHAR(512),
    status          VARCHAR(20) NOT NULL CHECK (status IN ('passed','failed','error','skipped')),
    duration_ms     INT,
    failure_message TEXT,
    failure_type    VARCHAR(255),
    stdout          TEXT,
    stderr          TEXT,
    created_at      TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_test_results_job ON test_results(job_id);
CREATE INDEX IF NOT EXISTS idx_test_results_group ON test_results(job_group_id);
CREATE INDEX IF NOT EXISTS idx_test_results_status ON test_results(status) WHERE status != 'passed';

-- Approval gates
CREATE TABLE IF NOT EXISTS approval_gates (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_group_id    UUID NOT NULL REFERENCES job_groups(id) ON DELETE CASCADE,
    stage_config_id UUID REFERENCES stage_configs(id) ON DELETE CASCADE,
    status          VARCHAR(20) DEFAULT 'pending' CHECK (status IN ('pending','approved','rejected','timed_out')),
    required_role   VARCHAR(50) DEFAULT 'admin',
    requested_at    TIMESTAMPTZ DEFAULT now(),
    responded_at    TIMESTAMPTZ,
    responded_by    UUID REFERENCES users(id),
    timeout_minutes INT DEFAULT 60,
    comment         TEXT,
    UNIQUE(job_group_id, stage_config_id)
);
CREATE INDEX IF NOT EXISTS idx_approval_gates_pending ON approval_gates(status) WHERE status = 'pending';

-- Concurrency controls on repos
ALTER TABLE repos ADD COLUMN IF NOT EXISTS max_concurrent_builds INT DEFAULT 0;
ALTER TABLE repos ADD COLUMN IF NOT EXISTS cancel_superseded BOOLEAN DEFAULT false;

-- Pinned builds (protected from retention)
ALTER TABLE job_groups ADD COLUMN IF NOT EXISTS pinned BOOLEAN DEFAULT false;

-- Approval settings on stages
ALTER TABLE stage_configs ADD COLUMN IF NOT EXISTS approval_required BOOLEAN DEFAULT false;
