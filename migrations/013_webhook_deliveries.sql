-- Webhook delivery logs: one row per inbound webhook event
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id       UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event            VARCHAR(100) NOT NULL,
    status_code      INT,
    response_time_ms INT,
    error_message    TEXT,
    created_at       TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_whdeliveries_webhook ON webhook_deliveries(webhook_id, created_at DESC);
