#!/usr/bin/env bash
# Remove the commands and the launcher entry. Your data (data/, ABOUT.md, .env) is left alone.
set -Eeuo pipefail

DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
uv tool uninstall careertwin 2>/dev/null || echo "careertwin was not installed with uv"
rm -f "$DATA/applications/careertwin-interview.desktop" \
  "$DATA/icons/hicolor/scalable/apps/careertwin-interview.svg" \
  "$DATA/icons/hicolor/256x256/apps/careertwin-interview.png"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" || true
echo "Removed. Your data is untouched."
