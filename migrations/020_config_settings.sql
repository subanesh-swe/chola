CREATE TABLE IF NOT EXISTS config_settings (
    key         VARCHAR(255) PRIMARY KEY,
    value       TEXT NOT NULL,
    description TEXT,
    updated_at  TIMESTAMPTZ DEFAULT now(),
    updated_by  VARCHAR(255)
);
