-- Drop old CHECK constraint first so we can update to new status values
ALTER TABLE work_items DROP CONSTRAINT work_items_status_check;

-- Migrate existing data to new scheduler-compatible statuses
UPDATE work_items SET status = CASE
    WHEN status = 'open' THEN 'waiting'
    WHEN status = 'in_progress' THEN 'agent_in_progress'
    WHEN status IN ('pending_review','approved') THEN 'agent_completed'
    WHEN status = 'done' THEN 'human_approved'
    WHEN status = 'rejected' THEN 'human_rejected'
    WHEN status = 'cancelled' THEN 'closed'
    ELSE status
END;

-- Add new CHECK constraint
ALTER TABLE work_items ADD CONSTRAINT work_items_status_check
    CHECK (status IN ('waiting','agent_in_progress','agent_completed','human_approved','human_rejected','closed'));
ALTER TABLE work_items ALTER COLUMN status SET DEFAULT 'waiting';

-- Scheduler index: quickly find next waiting work item per agent
CREATE INDEX idx_work_items_scheduler
    ON work_items (assigned_agent_id, status, priority, created_at)
    WHERE status = 'waiting' AND assigned_agent_id IS NOT NULL;
