-- enjo migration 0001 — initial relational schema for the local task store.
-- Phase 1 (local-only). The SQLite DB is the source of truth for the UI.
-- Idempotent: safe to run on every startup.

CREATE TABLE IF NOT EXISTS tasks (
    id           TEXT    PRIMARY KEY NOT NULL,                       -- UUIDv7 as text
    title        TEXT    NOT NULL,
    notes        TEXT,                                               -- nullable
    status       TEXT    NOT NULL CHECK (status IN ('todo', 'in_progress', 'done')),
    priority     TEXT    NOT NULL CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    due_date     TEXT,                                               -- nullable, 'YYYY-MM-DD'
    project      TEXT,                                               -- nullable
    created_at   TEXT    NOT NULL,                                   -- RFC3339
    updated_at   TEXT    NOT NULL,                                   -- RFC3339, the LWW clock
    completed_at TEXT,                                               -- nullable, RFC3339
    deleted      INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    server_seq   INTEGER                                             -- nullable, Phase 3 cursor
);

-- Sync cursor: scan tasks by last-write-wins clock (Phase 3).
CREATE INDEX IF NOT EXISTS idx_tasks_updated_at ON tasks (updated_at);

-- Active-list / Today queries: only non-deleted rows, narrowed by status.
CREATE INDEX IF NOT EXISTS idx_tasks_active ON tasks (status) WHERE deleted = 0;
