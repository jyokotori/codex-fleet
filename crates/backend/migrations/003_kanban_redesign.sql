-- Drop parent_id and type columns from work_items
-- Add 'backlog' to status constraint
-- No backward compatibility needed

-- 1. Drop index that references parent_id
DROP INDEX IF EXISTS idx_work_items_parent;

-- 2. Drop columns
ALTER TABLE work_items DROP COLUMN IF EXISTS parent_id;
ALTER TABLE work_items DROP COLUMN IF EXISTS type;

-- 3. Replace status constraint to include 'backlog'
ALTER TABLE work_items DROP CONSTRAINT IF EXISTS work_items_status_check;
ALTER TABLE work_items ADD CONSTRAINT work_items_status_check
    CHECK (status IN ('backlog','waiting','agent_in_progress','agent_completed','agent_failed',
                      'human_approved','human_rejected','cancelled'));
