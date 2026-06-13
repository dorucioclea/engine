-- Cron overlap policies + skip accounting.
--
-- overlap_policy  — what to do when a fire is due while a previous run from
--                   this schedule is still active:
--                     allow (default), skip, buffer_one, cancel_previous.
-- skipped_fires   — occurrences skipped by the `skip` policy (monotonic).
-- last_skipped_at — when the `skip` policy last skipped an occurrence.
--
-- Previous runs are attributed via instance metadata: the cron loop stamps
-- `metadata.cron_schedule_id` on every instance it creates.
ALTER TABLE cron_schedules ADD COLUMN IF NOT EXISTS overlap_policy TEXT NOT NULL DEFAULT 'allow';
ALTER TABLE cron_schedules ADD COLUMN IF NOT EXISTS skipped_fires BIGINT NOT NULL DEFAULT 0;
ALTER TABLE cron_schedules ADD COLUMN IF NOT EXISTS last_skipped_at TIMESTAMPTZ;
