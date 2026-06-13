-- Dynamic task-queue routing rules: override the queue an external-worker task
-- lands on, evaluated at enqueue. Keyed by (tenant_id, handler_name) with an
-- optional match_queue (apply only when the task's declared queue equals this).
-- Highest priority wins; enabled=false disables a rule without deleting it.
-- Closes the per-tenant/per-handler queue routing gap (temporalio/temporal#1988).
CREATE TABLE IF NOT EXISTS queue_routing_rules (
    id             UUID PRIMARY KEY,
    tenant_id      TEXT NOT NULL,
    handler_name   TEXT NOT NULL,
    match_queue    TEXT,
    queue_override TEXT NOT NULL,
    priority       INTEGER NOT NULL DEFAULT 0,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_queue_routing_tenant_handler
    ON queue_routing_rules (tenant_id, handler_name, priority DESC);
