-- 0004 — Postgres-backed worker task queue (any worker can claim any task).
--
-- Tasks are enqueued by the scheduler or API; workers claim rows with
-- FOR UPDATE SKIP LOCKED so multiple replicas process work concurrently.

CREATE TYPE worker_task_status AS ENUM ('pending', 'running', 'completed', 'failed');

CREATE TABLE worker_tasks (
    id           uuid                PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid                NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    task_type    text                NOT NULL,
    payload      jsonb               NOT NULL DEFAULT '{}',
    dedupe_key   text                NOT NULL DEFAULT '',
    status       worker_task_status  NOT NULL DEFAULT 'pending',
    priority     smallint            NOT NULL DEFAULT 0,
    run_at       timestamptz         NOT NULL DEFAULT now(),
    locked_at    timestamptz,
    locked_by    text,
    attempts     smallint            NOT NULL DEFAULT 0,
    max_attempts smallint            NOT NULL DEFAULT 3,
    last_error   text,
    created_at   timestamptz         NOT NULL DEFAULT now(),
    updated_at   timestamptz         NOT NULL DEFAULT now(),
    finished_at  timestamptz,
    CONSTRAINT worker_tasks_attempts_nonneg CHECK (attempts >= 0),
    CONSTRAINT worker_tasks_max_attempts_positive CHECK (max_attempts > 0)
);

-- One in-flight or queued task per tenant + dedupe key (e.g. ingest_usgs_live:usgs).
CREATE UNIQUE INDEX idx_worker_tasks_dedupe
    ON worker_tasks (tenant_id, dedupe_key)
    WHERE status IN ('pending', 'running');

-- Fast claim scan for pending tasks ready to run.
CREATE INDEX idx_worker_tasks_claim
    ON worker_tasks (run_at ASC, priority DESC)
    WHERE status = 'pending';
