#!/usr/bin/env bash
# Regenerate .sqlx/ offline query cache against a temporary PostgreSQL container.
# Run this after changing any SQL queries, before doing a Docker build.
set -euo pipefail

CONTAINER=codex-fleet-pg-prepare
PG_IMAGE=pgvector/pgvector:pg17
PG_USER=codexfleet
PG_PASSWORD=codexfleet
PG_DB=codexfleet
PG_PORT=5433  # use 5433 to avoid collision with any local postgres on 5432

export DATABASE_URL="postgres://${PG_USER}:${PG_PASSWORD}@localhost:${PG_PORT}/${PG_DB}?sslmode=disable"

cleanup() {
    echo "→ Removing temporary postgres container..."
    docker rm -f "$CONTAINER" 2>/dev/null || true
}
trap cleanup EXIT

echo "→ Starting temporary postgres (${PG_IMAGE})..."
docker run -d \
    --name "$CONTAINER" \
    -e POSTGRES_USER="$PG_USER" \
    -e POSTGRES_PASSWORD="$PG_PASSWORD" \
    -e POSTGRES_DB="$PG_DB" \
    -p "${PG_PORT}:5432" \
    "$PG_IMAGE"

echo "→ Waiting for postgres to be ready..."
until docker exec "$CONTAINER" pg_isready -U "$PG_USER" -d "$PG_DB" -q; do
    sleep 0.5
done

echo "→ Running migrations..."
sqlx migrate run --source crates/backend/migrations

echo "→ Generating .sqlx/ offline cache..."
cargo sqlx prepare --workspace

echo ""
echo "✓ Done. Commit the updated .sqlx/ files."
