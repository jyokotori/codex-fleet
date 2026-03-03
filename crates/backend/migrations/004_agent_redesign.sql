-- 1. New codex_configs table (one record = config.toml + auth.json combined)
CREATE TABLE codex_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    config_toml TEXT NOT NULL DEFAULT '',
    auth_json TEXT NOT NULL DEFAULT '',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 2. Rebuild agents table with new fields
PRAGMA foreign_keys=OFF;

CREATE TABLE agents_new (
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
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO agents_new (id, name, server_id, git_repo, git_branch, git_auth_type,
    git_username, git_password_encrypted, cli_type, docker_image,
    docker_container_name, tmux_session, workdir, status, created_at)
SELECT id, name, server_id, git_repo, git_branch, git_auth_type,
    git_username, git_password_encrypted, cli_type, docker_image,
    docker_container_name, tmux_session, workdir, status, created_at
FROM agents;

DROP TABLE agents;
ALTER TABLE agents_new RENAME TO agents;

PRAGMA foreign_keys=ON;
