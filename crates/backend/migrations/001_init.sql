CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS users (
    id TEXT NOT NULL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    password_encrypted TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS company_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    cli_type TEXT NOT NULL CHECK(cli_type IN ('claude','codex')),
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS servers (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    ip TEXT NOT NULL,
    port BIGINT NOT NULL DEFAULT 22,
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL CHECK(auth_type IN ('passwordless','password','key')),
    password_encrypted TEXT,
    ssh_key_content TEXT,
    status TEXT NOT NULL DEFAULT 'unknown',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    server_id TEXT NOT NULL REFERENCES servers(id),
    git_repo TEXT NOT NULL,
    git_branch TEXT NOT NULL DEFAULT 'main',
    git_auth_type TEXT NOT NULL CHECK(git_auth_type IN ('passwordless','https_password','ssh_key')),
    git_username TEXT,
    git_password_encrypted TEXT,
    cli_type TEXT NOT NULL CHECK(cli_type IN ('claude','codex')),
    company_config_id TEXT REFERENCES company_configs(id),
    docker_image TEXT NOT NULL DEFAULT 'ubuntu:22.04',
    docker_container_name TEXT,
    tmux_session TEXT NOT NULL DEFAULT 'main',
    workdir TEXT NOT NULL DEFAULT '/workspace',
    status TEXT NOT NULL DEFAULT 'stopped',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS notification_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    config_json TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    events_json TEXT NOT NULL DEFAULT '["task_completed","task_failed"]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
