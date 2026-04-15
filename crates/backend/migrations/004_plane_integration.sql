-- Migration 004: Plane Integration
-- Adds: user email, agent groups, plane bindings, plane task queue

-- 1. Add email to users
ALTER TABLE users ADD COLUMN email TEXT NOT NULL DEFAULT '';

-- 2. Agent groups
CREATE TABLE agent_groups (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE agent_group_members (
    group_id TEXT NOT NULL REFERENCES agent_groups(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, agent_id)
);

-- 3. Plane bindings (Plane project → agent group)
CREATE TABLE plane_bindings (
    id TEXT NOT NULL PRIMARY KEY,
    plane_project_id TEXT NOT NULL,
    plane_project_name TEXT NOT NULL,
    plane_project_identifier TEXT NOT NULL DEFAULT '',
    agent_group_id TEXT NOT NULL REFERENCES agent_groups(id),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 4. Plane task queue (webhook-received issues waiting for dispatch)
CREATE TABLE plane_tasks (
    id TEXT NOT NULL PRIMARY KEY,
    plane_issue_id TEXT NOT NULL,
    plane_project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    priority TEXT NOT NULL DEFAULT 'none',
    assignee_email TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    agent_id TEXT REFERENCES agents(id),
    task_id TEXT REFERENCES tasks(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
