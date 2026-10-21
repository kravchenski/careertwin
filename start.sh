#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOST="${BACKEND_HOST:-127.0.0.1}"
PORT="${BACKEND_PORT:-8000}"

cd "$ROOT_DIR"

if [[ "${1:-}" == "--docker" ]]; then
  exec docker compose up --build
fi

command -v uv >/dev/null 2>&1 || { echo "uv is required" >&2; exit 1; }

uv sync --extra rendering
env_file_args=()
[[ -f .env ]] && env_file_args=(--env-file .env)

echo "CareerTwin API on http://${HOST}:${PORT} (docs: http://${HOST}:${PORT}/docs)"
exec uv run uvicorn careertwin.api.app:app "${env_file_args[@]}" --host "$HOST" --port "$PORT" --reload
