# Codex Fleet

> ⚠️ **Work in progress — actively vibe coding, bugs expected.**

A web dashboard for managing multiple AI coding agents (Codex etc.) running across your servers or locally. Open your browser, create agents, send tasks, watch them work.

[中文文档 →](./README_CN.md)

---

## TODO List

1. Support skill/MCP configuration from multiple sources.
2. Improve the notification module (standalone notifications + third-party integrations).
3. Add a requirement menu and dispatch requirements to agents (standalone or integrated with Jira).
4. Build a simple web IDE with `codex: app server`.

> Note: Rust is used because Codex is built with Rust, and this project is also for learning it. Development speed depends on how fast my token refreshes (lol).

---

## Quick Start

```bash
git clone git@github.com:jyokotori/codex-fleet.git
cd codex-fleet
cp .env.example .env   # edit CODEX_MASTER_KEY before going to production

docker compose up -d
```

Open **http://localhost:3000**

Default admin login: **`codex` / `codex`**

> `.env` is gitignored. The bundled PostgreSQL container starts automatically when `COMPOSE_PROFILES=bundled-db` (the default).
> To use an external database, set `COMPOSE_PROFILES=` (empty) and update `DATABASE_URL` in `.env`.

---

## What can it do

### Servers
Add your remote VMs and test SSH connectivity with one click. Supports passwordless SSH, password auth, and SSH key. Once connected, all agents on that server run through it automatically.

### Agents
Create an agent by picking a server (or run locally on this machine — no SSH needed), choosing your CLI tool (Codex for now, more coming), and optionally pointing it at a Git repo. The dashboard provisions everything: pulls the repo, spins up a Docker container, sets up a tmux session inside it.

Each agent has its own:
- **Codex Config** — attach a `config.toml` + `auth.json` bundle so the agent has its credentials and settings ready
- **AGENTS.md** — inject shared project instructions into the agent's workspace
- **Docker Config** — customize port mappings, env vars, volume mounts, and init scripts

### Tasks
Open an agent, type a task, hit Send. The task goes straight into the agent's tmux session. You can see all past tasks and their status on the same page.

### Live Logs & Terminal
- **Logs tab** — real-time output from the agent's tmux session, auto-scrolling
- **Terminal tab** — full interactive terminal, type commands directly into the container

### Config Management
Store reusable configs in one place and attach them to any agent:
- **Codex Configs** — group `config.toml` and `auth.json` together as a named bundle
- **AGENTS.md** — shared instruction files for your agents
- **Docker Configs** — reusable Docker run configurations (ports, volumes, env vars, init scripts)

### Notifications
Set up webhooks to get notified when tasks complete or fail.

### User & Access Management
- JWT access token + refresh token
- Role-based access control (RBAC) with fine-grained permissions
- Admin-only user management: create user, reset password, enable/disable, unlock
- User self-service: change own password

---

## Architecture Evolution

The backend follows this evolution order:

1. IAM
2. Config Center
3. Servers + Agent Runtime
4. Notification Center

---

## Build from source

**Prerequisites:** Rust 1.88+, Node 20+, Docker, `sqlx-cli`

```bash
# Install sqlx-cli with PostgreSQL support
cargo install sqlx-cli --no-default-features --features native-tls,postgres

# Start a local postgres and generate the sqlx offline cache
./scripts/prepare-sqlx.sh

# Build frontend
cd frontend && npm install && npm run build && cd ..

# Run (requires a running postgres)
export DATABASE_URL=postgres://codexfleet:codexfleet@localhost:5432/codexfleet
cargo run -p backend
```

Open **http://localhost:3000**

---

## License

Apache 2.0
