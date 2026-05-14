-- Add per-binding notification configs so Plane-dispatched tasks can fire
-- the same task notifications (e.g. DingTalk) as user-created tasks on
-- agent_in_progress / agent_completed / agent_failed.
ALTER TABLE plane_bindings
    ADD COLUMN notification_ids TEXT NOT NULL DEFAULT '[]';
