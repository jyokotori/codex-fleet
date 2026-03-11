-- Add user_id to agents for ownership/visibility filtering
ALTER TABLE agents ADD COLUMN user_id TEXT REFERENCES users(id);
