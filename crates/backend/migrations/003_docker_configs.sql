-- Docker run configurations
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
