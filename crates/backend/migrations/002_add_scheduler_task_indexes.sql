-- Speed up scheduler idle-agent checks:
--   NOT EXISTS (
--     SELECT 1 FROM tasks
--     WHERE agent_id = a.id AND status = 'agent_in_progress'
--   )
CREATE INDEX idx_tasks_agent_in_progress
    ON tasks(agent_id)
    WHERE status = 'agent_in_progress';
