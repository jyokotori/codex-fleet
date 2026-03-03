# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`codex-fleet` is a WebUI-based orchestration center for managing Claude/Codex AI programming agents distributed across multiple remote VMs.

## Tech Stack

- **Backend**: Rust + Axum 0.8 + tokio, PostgreSQL via sqlx, async-ssh2-tokio for SSH, rust-embed for SPA
- **Frontend**: React 19 + TypeScript + Vite + Tailwind CSS + xterm.js (via @xterm/xterm)
- **Deploy**: Single Docker container (multi-stage build)

## Repository

- **Remote**: `git@github.com:jyokotori/codex-fleet.git`
- **License**: Apache 2.0

## Build Commands

### Frontend
```bash
cd frontend
npm install        # or pnpm install
npm run dev        # dev server at :5173 (proxies /api and /ws to :3000)
npm run build      # outputs to crates/backend/frontend-dist/
```

### Backend
```bash
# Requires a running PostgreSQL instance and DATABASE_URL for sqlx compile-time checks
export DATABASE_URL=postgres://codexfleet:codexfleet@localhost:5432/codexfleet
cargo build
cargo run          # serves on :3000
```

### Docker (recommended for local dev)
```bash
cp .env.example .env          # first time only — edit CODEX_MASTER_KEY
docker compose up --build     # starts postgres + app at :3000
docker compose logs -f app    # tail logs
```

### External PostgreSQL
```bash
# Edit .env: set COMPOSE_PROFILES= (empty) and DATABASE_URL to your external DB
docker compose up -d          # only app starts
```

## Key Directories

```
crates/backend/src/
  api/         - REST handlers (auth, servers, agents, tasks, configs, notifications)
  ssh/         - SSH client pool + tmux helpers
  ws/          - WebSocket log stream + terminal relay
  crypto.rs    - AES-256-GCM encrypt/decrypt
  main.rs      - Axum router

frontend/src/
  pages/       - Login, Register, Dashboard, Servers, Agents, AgentDetail, CompanyConfigs, Notifications
  components/  - LogStream, Terminal (xterm.js)
  lib/         - api.ts, i18n.ts, I18nContext.tsx, auth.ts
  hooks/       - useWebSocket.ts, useI18n.ts
```

## SQLx Offline Mode

The `.sqlx/` directory contains pre-generated query metadata for offline compilation (needed for Docker builds).
If you change SQL queries, regenerate with:
```bash
# Start a temporary postgres container
docker run -d --name pg-temp \
  -e POSTGRES_PASSWORD=codexfleet -e POSTGRES_USER=codexfleet \
  -e POSTGRES_DB=codexfleet -p 5432:5432 pgvector/pgvector:pg17

export DATABASE_URL=postgres://codexfleet:codexfleet@localhost:5432/codexfleet
sqlx database create
sqlx migrate run --source crates/backend/migrations
cargo sqlx prepare --workspace

docker rm -f pg-temp
```
Commit the regenerated `.sqlx/` files.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `3000` | HTTP listen port |
| `DATABASE_URL` | `postgres://codexfleet:codexfleet@localhost:5432/codexfleet` | PostgreSQL URL |
| `CODEX_MASTER_KEY` | `change-me-in-production` | AES-256 encryption key |
| `RUST_LOG` | `backend=info,tower_http=info` | Log level filter |

## Default Credentials

Username: `codex` / Password: `codex` (created at startup if not present)
