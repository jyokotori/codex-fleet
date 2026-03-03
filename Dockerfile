# Stage 1: Build frontend
FROM node:20-alpine AS frontend
WORKDIR /app/frontend
RUN npm install -g pnpm
COPY frontend/package.json frontend/pnpm-lock.yaml* ./
# Fallback to npm if no pnpm lock file
RUN if [ -f pnpm-lock.yaml ]; then pnpm install --frozen-lockfile; else npm install; fi
COPY frontend/ .
RUN if [ -f pnpm-lock.yaml ]; then pnpm build; else npm run build; fi

# Stage 2: Build Rust backend
FROM rust:1.88-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
COPY .sqlx/ .sqlx/
COPY --from=frontend /app/crates/backend/frontend-dist crates/backend/frontend-dist/
# Build with release profile (sqlx offline mode — no DB needed at compile time)
ENV SQLX_OFFLINE=true
RUN cargo build --release -p backend

# Stage 3: Runtime
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y ca-certificates openssh-client docker.io && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/backend /usr/local/bin/backend
RUN mkdir -p /data
EXPOSE 3000
ENV DATABASE_URL=sqlite:///data/codex-fleet.db
ENV CODEX_MASTER_KEY=change-me-in-production
ENV PORT=3000
CMD ["/usr/local/bin/backend"]
