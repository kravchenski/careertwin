#!/usr/bin/env bash
# Install CareerTwin for the current user: the careertwin, careertwin-mcp and careertwin-interview
# commands plus an app-launcher entry with an icon. Works on CachyOS and any Arch-based system.
#
# The install is editable: the commands run the code of THIS checkout, so a `git pull` or a branch
# switch takes effect at once. Re-run the script after dependencies change. Undo with uninstall.sh.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor/scalable/apps"
ICONS_PNG="$DATA/icons/hicolor/256x256/apps"

missing=0
need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing: $1 -> $2" >&2
    missing=1
  fi
}
need uv "sudo pacman -S uv  (or https://docs.astral.sh/uv/)"
need parec "sudo pacman -S libpulse pipewire-pulse   (captures the call audio)"
need pactl "sudo pacman -S libpulse"
[[ "$missing" == 0 ]] || exit 1

EXTRAS="interview,rendering"
# CTranslate2 needs the CUDA 12 runtime; the system CUDA on Arch is newer, so the pip wheels are used.
if [[ -z "${CAREERTWIN_NO_GPU:-}" ]] && command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi -L >/dev/null 2>&1; then
  EXTRAS="$EXTRAS,gpu"
  echo "NVIDIA GPU found: adding the CUDA 12 libraries (about 1.3 GB) for much faster speech recognition."
  echo "Set CAREERTWIN_NO_GPU=1 to skip them."
fi

echo "Installing the commands with uv (editable, Python 3.14, extras: $EXTRAS)..."
uv tool install --editable --force --python 3.14 "$ROOT[$EXTRAS]"
BIN="$(uv tool dir --bin)"

install -Dm644 "$ROOT/packaging/careertwin-interview.svg" "$ICONS/careertwin-interview.svg"
install -Dm644 "$ROOT/packaging/careertwin-interview.png" "$ICONS_PNG/careertwin-interview.png"
mkdir -p "$APPS"
# An absolute icon path works even in a launcher that started before ~/.local/share/icons existed
# and cached the icon directories (it would otherwise show a generic gear until restarted).
sed -e "s|@BIN@|$BIN|" -e "s|@ICON@|$ICONS_PNG/careertwin-interview.png|" \
  "$ROOT/packaging/careertwin-interview.desktop" > "$APPS/careertwin-interview.desktop"
chmod 644 "$APPS/careertwin-interview.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$DATA/icons/hicolor" 2>/dev/null || true

echo
echo "Installed:"
echo "  $BIN/careertwin-interview   (also careertwin, careertwin-mcp)"
echo "  $APPS/careertwin-interview.desktop"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "note: $BIN is not on your PATH; add it (fish: fish_add_path $BIN)" ;;
esac
command -v grim >/dev/null 2>&1 || command -v spectacle >/dev/null 2>&1 \
  || echo "note: the Screenshot button needs grim: sudo pacman -S grim"
echo
echo "Run: careertwin-interview            (or open 'CareerTwin Interview' from your launcher)"
echo "Check the setup: careertwin-interview --doctor"
