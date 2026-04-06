-- Registration tokens: admin creates, worker exchanges for permanent token
CREATE TABLE IF NOT EXISTS worker_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    scope VARCHAR(20) DEFAULT 'shared',
    created_by VARCHAR(255),
    expires_at TIMESTAMPTZ,
    max_uses INT DEFAULT 0,
    uses INT DEFAULT 0,
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Enhance workers table with auth fields
ALTER TABLE workers ADD COLUMN IF NOT EXISTS worker_token_hash VARCHAR(64);
ALTER TABLE workers ADD COLUMN IF NOT EXISTS registration_token_id UUID;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS approved BOOLEAN DEFAULT true;
ALTER TABLE workers ADD COLUMN IF NOT EXISTS description TEXT;

-- Label group configs: shared config per label set
CREATE TABLE IF NOT EXISTS label_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    match_labels TEXT[] NOT NULL DEFAULT '{}',
    env_vars JSONB DEFAULT '{}',
    pre_script TEXT,
    max_concurrent_jobs INT DEFAULT 0,
    capabilities TEXT[] DEFAULT '{}',
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

-- Indexes for worker_tokens lookup
CREATE INDEX IF NOT EXISTS idx_worker_tokens_hash ON worker_tokens (token_hash);
CREATE INDEX IF NOT EXISTS idx_worker_tokens_active ON worker_tokens (active) WHERE active = true;

-- Index for worker auth lookup
CREATE INDEX IF NOT EXISTS idx_workers_token_hash ON workers (worker_token_hash) WHERE worker_token_hash IS NOT NULL;
