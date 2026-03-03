# Codex Fleet

> ⚠️ **Work in progress — actively vibe coding, bugs expected.**

A web dashboard for managing multiple AI coding agents (Codex etc.) running across your servers or locally. Open your browser, create agents, send tasks, watch them work.

[中文文档 →](./README_CN.md)

---

## Quick Start

```bash
git clone git@github.com:jyokotori/codex-fleet.git
cd codex-fleet

docker compose up -d
```

Open **http://localhost:3000**

Default login: **`codex` / `codex`**

> In production, set a real secret key:
> ```bash
> CODEX_MASTER_KEY=your-secret docker compose up -d
> ```

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

---

## Build from source

**Prerequisites:** Rust 1.88+, Node 20+, `sqlx-cli`

```bash
cargo install sqlx-cli --no-default-features --features sqlite

mkdir -p data
DATABASE_URL="sqlite://./data/codex-fleet.db" sqlx database create
DATABASE_URL="sqlite://./data/codex-fleet.db" sqlx migrate run --source crates/backend/migrations

cd frontend && npm install && npm run build && cd ..

DATABASE_URL="sqlite://./data/codex-fleet.db" cargo run -p backend
```

Open **http://localhost:3000**

---

## License

Apache 2.0
