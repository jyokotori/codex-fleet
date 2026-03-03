ALTER TABLE agents ADD COLUMN container_id TEXT;
ALTER TABLE agents ADD COLUMN provision_log TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN tmux_window TEXT;
