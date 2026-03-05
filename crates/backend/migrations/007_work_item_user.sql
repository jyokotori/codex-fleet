ALTER TABLE work_items ADD COLUMN assigned_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;
CREATE INDEX idx_work_items_user ON work_items(assigned_user_id);
