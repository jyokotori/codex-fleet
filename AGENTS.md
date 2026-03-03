# AGENTS.md

This document defines engineering collaboration rules for the `codex-fleet` repository.
Use it as the primary implementation guide for human contributors and coding agents.

## Project Overview

`codex-fleet` is a web control plane for managing AI coding agents across remote servers.
The backend architecture is now domain-oriented and split into multiple Rust crates.

## Tech Stack

- Backend: Rust + Axum + Tokio + SQLx + PostgreSQL
- Frontend: React + TypeScript + Vite + Tailwind
- Deployment: Docker Compose (recommended)

## Common Commands

### Docker (recommended)

```bash
cp .env.example .env
docker compose up -d --build
docker compose logs -f app
```

### Backend (local)

```bash
export DATABASE_URL=postgres://codexfleet:codexfleet@localhost:5432/codexfleet
cargo run -p backend
```

### Frontend (local)

```bash
cd frontend
npm install
npm run dev
```

## Module Ownership and Boundaries

- `crates/backend`: Composition root (bootstrap, DI wiring, router assembly, static assets)
- `crates/shared_kernel`: Shared context, error model, auth context, shared config
- `crates/iam`: Authentication, authorization, user lifecycle, audit logs
- `crates/config_center`: `company_configs`, `codex_configs`, `docker_configs`
- `crates/runtime_agent`: `servers`, `agents`, `tasks`, `ws`, SSH runtime orchestration
- `crates/notification_center`: Notification configs and delivery

Cross-module usage must go through public APIs/traits.
Do not directly access private internals across domains.

## Layering Rules (api/application/domain/infrastructure)

Each business crate should follow these layers:

- `api`: HTTP/WS handlers, DTO mapping, request validation
- `application`: Use-case orchestration, transaction boundaries, permission checks
- `domain`: Entities, value objects, domain rules
- `infrastructure`: DB access, external adapters (SSH/HTTP/message)

Rules:

- `api` should not contain large business orchestration logic
- `application` should not depend on frontend DTOs
- `domain` should not depend on axum/sqlx
- `infrastructure` should not leak low-level implementation details upward

## Route Ownership

- `iam`
  - `POST /api/auth/login`
  - `POST /api/auth/refresh`
  - `POST /api/auth/logout`
  - `GET /api/me`
  - `PUT /api/me/password`
  - `GET /api/admin/users`
  - `POST /api/admin/users`
  - `POST /api/admin/users/{id}/reset-password`
  - `PATCH /api/admin/users/{id}/status`
  - `POST /api/admin/users/{id}/unlock`
- `config_center`
  - `GET/POST/PUT/DELETE /api/configs...`
  - `GET/POST/PUT/DELETE /api/codex-configs...`
  - `GET/POST/PUT/DELETE /api/docker-configs...`
- `runtime_agent`
  - `GET/POST/PUT/DELETE /api/servers...`
  - `GET/POST/PUT/DELETE /api/agents...`
  - `GET/POST /api/agents/{id}/tasks`
  - `GET /api/tasks/{id}`
  - `GET /ws/agents/{id}/logs|terminal|provision`
- `notification_center`
  - `GET/POST/PUT/DELETE /api/notifications...`

## Key Paths

- `crates/backend/migrations/`: DB migrations
- `crates/backend/src/main.rs`: bootstrap and router composition
- `frontend/src/lib/api.ts`: frontend API wrapper
- `frontend/src/components/Layout.tsx`: top navigation (includes admin menu)

## Database Migration Rules

- All schema changes must be committed under `crates/backend/migrations/*.sql`
- Migration files must be replayable on a fresh database
- IAM-related schema changes must keep role/permission/admin seed logic aligned

## SQLx Offline Metadata

To refresh `.sqlx/` offline metadata, run:

```bash
./scripts/prepare-sqlx.sh
```

## Test Gates

Before submitting changes, at minimum run:

- `cargo check -p backend`
- `cd frontend && npm run build`

For IAM/authorization changes, also verify:

- Admin menu is visible only to admin users
- Non-admin access to admin APIs returns 403
- Users can only change their own passwords
- Admin can reset password / disable / unlock users

## Prohibited Changes

- Do not place large orchestration logic directly inside handlers
- Do not query/write other domain tables directly across boundaries
- Do not scatter SSH/external execution details across `api` layer
- Do not revert unrelated changes in the working tree

## Documentation Sync Rule (README / README_CN)

Any user-visible feature change must update both:

- `README.md`
- `README_CN.md`

Documentation requirements:

- Distinguish `Current` (implemented) vs `Planned` (upcoming)
- Keep English/Chinese structures aligned
- Do not present not-yet-released features as already available

## Architecture Evolution Order

Follow this sequence and avoid cross-phase mixing:

1. IAM
2. Config Center
3. Server + Agent Runtime
4. Notification Center
