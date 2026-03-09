-- Rebuild tasks table with desired column order and new work_item_id field.
-- Safe because table has no production data.
ALTER TABLE work_items DROP CONSTRAINT IF EXISTS work_items_execution_id_fkey;
DROP TABLE IF EXISTS tasks;
CREATE TABLE tasks (
    id            TEXT PRIMARY KEY NOT NULL,
    agent_id      TEXT NOT NULL REFERENCES agents(id),
    title         TEXT NOT NULL DEFAULT '',
    description   TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    work_item_id  TEXT,
    task_dir      TEXT NOT NULL DEFAULT '',
    task_log      TEXT NOT NULL DEFAULT '',
    result_md     TEXT NOT NULL DEFAULT '',
    thread_id     TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at    TIMESTAMPTZ,
    completed_at  TIMESTAMPTZ
);

-- Re-add the FK from work_items.execution_id → tasks.id
ALTER TABLE work_items ADD CONSTRAINT work_items_execution_id_fkey
    FOREIGN KEY (execution_id) REFERENCES tasks(id);
