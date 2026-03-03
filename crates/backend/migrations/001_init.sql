CREATE EXTENSION IF NOT EXISTS vector;

-- IAM (v2)
CREATE TABLE users (
    id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until TIMESTAMPTZ,
    last_login_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE roles (
    id TEXT NOT NULL PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE permissions (
    id TEXT NOT NULL PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    resource TEXT NOT NULL,
    action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE role_permissions (
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id TEXT NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE refresh_tokens (
    id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    jti TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE audit_logs (
    id TEXT NOT NULL PRIMARY KEY,
    actor_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Config center
CREATE TABLE company_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'config_file',
    cli_type TEXT NOT NULL CHECK (cli_type IN ('claude','codex','claude_code','gemini','gemini_cli','opencode')),
    file_type TEXT,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE codex_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    config_toml TEXT NOT NULL DEFAULT '',
    auth_json TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE docker_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    port_mappings TEXT NOT NULL DEFAULT '[]',
    env_vars TEXT NOT NULL DEFAULT '[]',
    volume_mappings TEXT NOT NULL DEFAULT '[]',
    init_script TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Runtime agent
CREATE TABLE servers (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    ip TEXT NOT NULL,
    port BIGINT NOT NULL DEFAULT 22,
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL CHECK (auth_type IN ('passwordless','password','key')),
    password_encrypted TEXT,
    ssh_key_content TEXT,
    os_type TEXT NOT NULL DEFAULT 'linux',
    status TEXT NOT NULL DEFAULT 'unknown',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

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
    container_id TEXT,
    tmux_session TEXT NOT NULL DEFAULT 'main',
    workdir TEXT NOT NULL DEFAULT '/workspace',
    use_docker BOOLEAN NOT NULL DEFAULT TRUE,
    status TEXT NOT NULL DEFAULT 'stopped',
    provision_log TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tasks (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    tmux_window TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

-- Notification center
CREATE TABLE notification_configs (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    config_json TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    events_json TEXT NOT NULL DEFAULT '["task_completed","task_failed"]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_status ON users(status);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_exp ON refresh_tokens(expires_at);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at);
