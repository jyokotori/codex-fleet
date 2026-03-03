# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`codex-fleet` is a WebUI-based orchestration center for managing Claude/Codex AI programming agents distributed across multiple remote VMs.

## Tech Stack

- **Backend**: Rust + Axum 0.8 + tokio, SQLite via sqlx, async-ssh2-tokio for SSH, rust-embed for SPA
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
# Requires DATABASE_URL for sqlx compile-time query checks
export DATABASE_URL=sqlite:///tmp/codex-fleet-dev.db
cargo build
cargo run          # serves on :3000
```

### Docker
```bash
docker compose up --build   # builds and starts at :3000
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

The `.sqlx/` directory contains pre-generated query metadata for offline compilation.
If you change SQL queries, regenerate with:
```bash
export DATABASE_URL=sqlite:///tmp/codex-fleet-dev.db
cargo sqlx migrate run
cargo sqlx prepare --workspace
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `3000` | HTTP listen port |
| `DATABASE_URL` | `sqlite:///data/codex-fleet.db` | SQLite path |
| `CODEX_MASTER_KEY` | `dev-master-key-change-in-prod` | AES-256 encryption key |

## Default Credentials

Username: `codex` / Password: `codex` (created at startup if not present)
