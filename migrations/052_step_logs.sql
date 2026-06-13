-- Step-scoped logs: log lines for a step, keyed by (instance_id, block_id).
-- Populated by external workers (logs attached on complete/fail) and by the
-- engine's in-process capture layer scoped to the `orch8.step` span. Surfaced
-- per-execution in the dashboard Logs view.
CREATE TABLE IF NOT EXISTS step_logs (
    id          UUID PRIMARY KEY,
    instance_id UUID NOT NULL,
    block_id    TEXT NOT NULL,
    ts          TIMESTAMPTZ NOT NULL,
    level       TEXT NOT NULL,
    message     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_step_logs_instance ON step_logs (instance_id, ts);
