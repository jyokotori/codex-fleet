PRAGMA foreign_keys=OFF;

CREATE TABLE company_configs_v2 (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'config_file',
    cli_type TEXT NOT NULL DEFAULT 'codex',
    file_type TEXT,
    content TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO company_configs_v2 (id, name, category, cli_type, content, created_at, updated_at)
  SELECT id, name, 'config_file', cli_type, content, created_at, updated_at FROM company_configs;

DROP TABLE company_configs;

ALTER TABLE company_configs_v2 RENAME TO company_configs;

PRAGMA foreign_keys=ON;
