-- Add notification_ids to projects for default notification inheritance
ALTER TABLE projects ADD COLUMN notification_ids TEXT NOT NULL DEFAULT '[]';
