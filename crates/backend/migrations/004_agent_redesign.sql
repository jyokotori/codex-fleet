-- 1. New codex_configs table (one record = config.toml + auth.json combined)
CREATE TABLE codex_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    config_toml TEXT NOT NULL DEFAULT '',
    auth_json TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. Rebuild agents table with new fields
-- Drop old agents table and recreate with new schema
DROP TABLE IF EXISTS agents CASCADE;

CREATE TABLE agents (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    server_id TEXT NOT NULL DEFAULT '__local__',
    git_repo TEXT NOT NULL DEFAULT '',
    git_branch TEXT NOT NULL DEFAULT 'main',
    git_auth_type TEXT NOT NULL DEFAULT 'none',
    git_username TEXT,
    git_password_encrypted TEXT,
    cli_type TEXT NOT NULL DEFAULT 'codex',
    codex_config_id TEXT REFERENCES codex_configs(id),
    agents_md_id TEXT REFERENCES company_configs(id),
    docker_config_id TEXT REFERENCES docker_configs(id),
    docker_image TEXT NOT NULL DEFAULT 'ubuntu:24.04',
    docker_container_name TEXT,
    tmux_session TEXT NOT NULL DEFAULT 'main',
    workdir TEXT NOT NULL DEFAULT '/workspace',
    status TEXT NOT NULL DEFAULT 'stopped',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
