-- tasks: add notification_ids, user_id, username
ALTER TABLE tasks ADD COLUMN notification_ids TEXT NOT NULL DEFAULT '[]';
ALTER TABLE tasks ADD COLUMN user_id TEXT;
ALTER TABLE tasks ADD COLUMN username TEXT NOT NULL DEFAULT '';

-- work_items: add assigned_username, notification_ids
ALTER TABLE work_items ADD COLUMN assigned_username TEXT NOT NULL DEFAULT '';
ALTER TABLE work_items ADD COLUMN notification_ids TEXT NOT NULL DEFAULT '[]';

-- notification_configs: migrate old event names to unified statuses
UPDATE notification_configs SET events_json = REPLACE(REPLACE(REPLACE(REPLACE(events_json,
  '"task_completed"', '"agent_completed"'),
  '"task_failed"', '"agent_failed"'),
  '"agent_started"', '"agent_in_progress"'),
  '"agent_stopped"', '"cancelled"');
ALTER TABLE notification_configs ALTER COLUMN events_json SET DEFAULT '["agent_completed","agent_failed"]';
