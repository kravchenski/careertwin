#!/usr/bin/env bash
# Copy the code to a server and (re)start CareerTwin there. It never copies a secret: put `.env` and
# `ABOUT.md` on the server yourself (docs/HOSTING.md, step 4).
#
#   scripts/deploy-remote.sh ubuntu@203.0.113.7 [directory on the server]
#   SSH_KEY=~/.ssh/aws.pem scripts/deploy-remote.sh ubuntu@203.0.113.7
set -Eeuo pipefail

TARGET="${1:?usage: scripts/deploy-remote.sh user@host [directory on the server]}"
DIR="${2:-careertwin}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SSH=(ssh -o StrictHostKeyChecking=accept-new)
[[ -n "${SSH_KEY:-}" ]] && SSH+=(-i "$SSH_KEY")

# Excluded paths are also protected from --delete on the server, so its data and secrets stay.
rsync -az --delete \
  --exclude '.git' --exclude '.venv' --exclude 'node_modules' --exclude 'dist' --exclude '__pycache__' \
  --exclude 'data' --exclude '.env' --exclude 'ABOUT.md' --exclude '*.pdf' \
  -e "${SSH[*]}" "$ROOT/" "$TARGET:$DIR/"

"${SSH[@]}" "$TARGET" "cd '$DIR' && for f in .env ABOUT.md; do
  [ -f \"\$f\" ] || { echo \"\$f is missing in $DIR on the server: see docs/HOSTING.md, step 4\" >&2; exit 1; }
done
chmod 600 .env ABOUT.md
docker compose up -d --build
docker compose ps"
