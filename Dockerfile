# syntax=docker/dockerfile:1.7

# Stage 1: Build frontend
FROM node:20-alpine AS frontend
WORKDIR /app/frontend
RUN corepack enable
COPY frontend/package.json frontend/package-lock.json* frontend/pnpm-lock.yaml* ./
RUN --mount=type=cache,target=/root/.npm,id=codex-fleet-npm-cache \
    --mount=type=cache,target=/root/.local/share/pnpm/store,id=codex-fleet-pnpm-cache \
    if [ -f pnpm-lock.yaml ]; then \
        pnpm install --frozen-lockfile; \
    elif [ -f package-lock.json ]; then \
        npm ci; \
    else \
        npm install; \
    fi
COPY frontend/ .
RUN if [ -f pnpm-lock.yaml ]; then pnpm build; else npm run build; fi

# Stage 2: Prepare Rust dependency recipe
FROM rust:1.88-slim AS chef
RUN apt-get update && apt-get install -y pkg-config libssl-dev && cargo install cargo-chef --locked && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock* ./
COPY crates/backend/Cargo.toml crates/backend/Cargo.toml
COPY crates/shared_kernel/Cargo.toml crates/shared_kernel/Cargo.toml
COPY crates/iam/Cargo.toml crates/iam/Cargo.toml
COPY crates/config_center/Cargo.toml crates/config_center/Cargo.toml
COPY crates/runtime_agent/Cargo.toml crates/runtime_agent/Cargo.toml
COPY crates/notification_center/Cargo.toml crates/notification_center/Cargo.toml
RUN mkdir -p crates/backend/src && printf 'fn main() {}\n' > crates/backend/src/main.rs && \
    for crate in shared_kernel iam config_center runtime_agent notification_center; do \
        mkdir -p "crates/${crate}/src"; \
        printf '// cargo-chef placeholder\n' > "crates/${crate}/src/lib.rs"; \
    done
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=codex-fleet-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=codex-fleet-cargo-git \
    --mount=type=cache,target=/app/target,id=codex-fleet-cargo-target \
    cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
COPY .sqlx/ .sqlx/
COPY --from=frontend /app/crates/backend/frontend-dist/ crates/backend/frontend-dist/
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=codex-fleet-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=codex-fleet-cargo-git \
    --mount=type=cache,target=/app/target,id=codex-fleet-cargo-target \
    cargo build --release -p backend && cp target/release/backend /tmp/backend

# Stage 3: Runtime
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y ca-certificates openssh-client docker.io && rm -rf /var/lib/apt/lists/*
COPY --from=builder /tmp/backend /usr/local/bin/backend
EXPOSE 3000
ENV PORT=3000
ENV CODEX_MASTER_KEY=change-me-in-production
ENV RUST_LOG=backend=info,tower_http=info
CMD ["/usr/local/bin/backend"]
