-- Unify work_item and task status values into a single set:
-- waiting, agent_in_progress, agent_completed, agent_failed,
-- human_approved, human_rejected, cancelled, closed

-- 1. Update task statuses to match the unified set
UPDATE tasks SET status = CASE
    WHEN status = 'pending'   THEN 'waiting'
    WHEN status = 'running'   THEN 'agent_in_progress'
    WHEN status = 'completed' THEN 'agent_completed'
    WHEN status = 'failed'    THEN 'agent_failed'
    WHEN status = 'cancelled' THEN 'cancelled'
    ELSE status
END;
ALTER TABLE tasks ALTER COLUMN status SET DEFAULT 'waiting';

-- 2. Update work_items: add agent_failed and cancelled to the allowed set
ALTER TABLE work_items DROP CONSTRAINT IF EXISTS work_items_status_check;
ALTER TABLE work_items ADD CONSTRAINT work_items_status_check
    CHECK (status IN ('waiting','agent_in_progress','agent_completed','agent_failed',
                      'human_approved','human_rejected','cancelled','closed'));
