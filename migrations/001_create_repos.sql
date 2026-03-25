CREATE TABLE IF NOT EXISTS repos (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_name     VARCHAR(255) NOT NULL UNIQUE,
    repo_url      TEXT NOT NULL,
    default_branch VARCHAR(100) DEFAULT 'main',
    enabled       BOOLEAN DEFAULT true,
    created_at    TIMESTAMPTZ DEFAULT now(),
    updated_at    TIMESTAMPTZ DEFAULT now()
);
