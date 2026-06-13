-- Worker control channel: control commands queued for a specific worker,
-- delivered via GET /workers/{id}/commands. The worker acts on pending commands
-- (drain / reload / ping) and acks them with DELETE /workers/commands/{id}.
CREATE TABLE IF NOT EXISTS worker_commands (
    id         UUID PRIMARY KEY,
    worker_id  TEXT NOT NULL,
    command    TEXT NOT NULL,
    payload    JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_worker_commands_worker
    ON worker_commands (worker_id, created_at);
