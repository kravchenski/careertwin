#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_HOST="${BACKEND_HOST:-127.0.0.1}"
BACKEND_PORT="${BACKEND_PORT:-8000}"
FRONTEND_HOST="${FRONTEND_HOST:-127.0.0.1}"
FRONTEND_PORT="${FRONTEND_PORT:-4200}"
LOG_DIR="${LOG_DIR:-/tmp/careertwin}"

mkdir -p "$LOG_DIR"

if [[ "${1:-}" == "--docker" ]]; then
  cd "$ROOT_DIR"
  exec docker compose up --build
fi

command -v python >/dev/null 2>&1 || { echo "python is required" >&2; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "npm is required" >&2; exit 1; }
command -v uv >/dev/null 2>&1 || { echo "uv is required" >&2; exit 1; }

uv sync --project "$ROOT_DIR" --extra rendering
mkdir -p "$ROOT_DIR/web/api/screenshots"
env_file_args=()
[[ -f "$ROOT_DIR/.env" ]] && env_file_args=(--env-file "$ROOT_DIR/.env")

if [[ ! -x "$ROOT_DIR/web/frontend/node_modules/.bin/ng" ]]; then
  echo "Installing frontend dependencies..."
  npm ci --prefix "$ROOT_DIR/web/frontend" --no-audit --no-fund
fi

backend_pid=""
frontend_pid=""

cleanup() {
  trap - EXIT INT TERM
  [[ -n "$frontend_pid" ]] && kill "$frontend_pid" 2>/dev/null || true
  [[ -n "$backend_pid" ]] && kill "$backend_pid" 2>/dev/null || true
  wait "$frontend_pid" "$backend_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "Starting backend on http://${BACKEND_HOST}:${BACKEND_PORT}"
(
  cd "$ROOT_DIR"
  export PYTHONPATH="$ROOT_DIR/web/api:$ROOT_DIR/src${PYTHONPATH:+:$PYTHONPATH}"
  exec "$ROOT_DIR/.venv/bin/python" -m uvicorn web.api.main:app \
    "${env_file_args[@]}" \
    --host "$BACKEND_HOST" \
    --port "$BACKEND_PORT" \
    --reload
) >"$LOG_DIR/backend.log" 2>&1 &
backend_pid=$!

for _ in {1..30}; do
  if curl -fsS "http://${BACKEND_HOST}:${BACKEND_PORT}/v1/health" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$backend_pid" 2>/dev/null; then
    echo "Backend failed to start. Log:" >&2
    tail -n 80 "$LOG_DIR/backend.log" >&2
    exit 1
  fi
  sleep 1
done

curl -fsS "http://${BACKEND_HOST}:${BACKEND_PORT}/v1/health" >/dev/null || {
  echo "Backend health check timed out. Log: $LOG_DIR/backend.log" >&2
  exit 1
}

echo "Starting frontend on http://${FRONTEND_HOST}:${FRONTEND_PORT}"
(
  cd "$ROOT_DIR/web/frontend"
  exec npm run start -- --host "$FRONTEND_HOST" --port "$FRONTEND_PORT"
) >"$LOG_DIR/frontend.log" 2>&1 &
frontend_pid=$!

for _ in {1..45}; do
  if curl -fsS "http://${FRONTEND_HOST}:${FRONTEND_PORT}" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$frontend_pid" 2>/dev/null; then
    echo "Frontend failed to start. Log:" >&2
    tail -n 80 "$LOG_DIR/frontend.log" >&2
    exit 1
  fi
  sleep 1
done

curl -fsS "http://${FRONTEND_HOST}:${FRONTEND_PORT}" >/dev/null || {
  echo "Frontend health check timed out. Log: $LOG_DIR/frontend.log" >&2
  exit 1
}

echo "CareerTwin is running"
echo "Frontend: http://${FRONTEND_HOST}:${FRONTEND_PORT}"
echo "Backend:  http://${BACKEND_HOST}:${BACKEND_PORT}"
echo "Logs:     $LOG_DIR"
echo "Press Ctrl+C to stop both services."

wait -n "$backend_pid" "$frontend_pid"
