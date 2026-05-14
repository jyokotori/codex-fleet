#!/usr/bin/env bash
# Pull latest code, rebuild only the app image, and restart the dev app container.
# Database is expected to be running elsewhere (configured via .env).
set -euo pipefail

cd "$(dirname "$0")/.."

COMPOSE_FILE=docker-compose.dev.yml

echo "→ git pull"
git pull --ff-only

echo "→ Rebuilding app image..."
docker compose -f "$COMPOSE_FILE" build app

echo "→ Restarting app container..."
docker compose -f "$COMPOSE_FILE" up -d app

echo ""
echo "✓ Done."
docker compose -f "$COMPOSE_FILE" ps
