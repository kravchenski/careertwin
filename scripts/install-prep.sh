#!/usr/bin/env bash
# Build and install CareerTwin Prep for the current user: the careertwin-prep program, its icon and a
# launcher entry. Needs Rust (https://rustup.rs) and, on Linux, the ALSA headers (alsa-lib).
#
#   ./scripts/install-prep.sh            the program with offline hearing and voice
#   LIGHT=1 ./scripts/install-prep.sh    without them: a smaller build that uses cloud or server engines
#
# Undo with ./scripts/install-prep.sh --uninstall
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor/512x512/apps"
PREFIX="${PREFIX:-$HOME/.local}"

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$PREFIX/bin/careertwin-prep" "$APPS/careertwin-prep.desktop" "$ICONS/careertwin-prep.png"
  echo "Removed. Your settings and sessions are kept in ~/.config/careertwin-prep and ~/.local/share/careertwin-prep."
  exit 0
fi

command -v cargo >/dev/null 2>&1 || { echo "missing: cargo -> https://rustup.rs" >&2; exit 1; }
if [[ "$(uname -s)" == "Linux" ]] && ! pkg-config --exists alsa 2>/dev/null; then
  echo "missing: the ALSA headers -> sudo pacman -S alsa-lib   (Debian and Ubuntu: sudo apt install libasound2-dev)" >&2
  exit 1
fi

FEATURES="local"
if [[ -n "${LIGHT:-}" ]]; then
  FEATURES="gui"
fi

echo "Building CareerTwin Prep (features: $FEATURES). The first build takes a while."
cargo install --path "$ROOT/prep" --locked --root "$PREFIX" --features "$FEATURES" --force

install -Dm644 "$ROOT/packaging/careertwin-prep.png" "$ICONS/careertwin-prep.png"
mkdir -p "$APPS"
sed -e "s|@BIN@|$PREFIX/bin|" -e "s|@ICON@|$ICONS/careertwin-prep.png|" \
  "$ROOT/packaging/careertwin-prep.desktop" > "$APPS/careertwin-prep.desktop"
chmod 644 "$APPS/careertwin-prep.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true

echo
echo "Installed: $PREFIX/bin/careertwin-prep"
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) echo "note: $PREFIX/bin is not on your PATH; add it (fish: fish_add_path $PREFIX/bin)" ;;
esac
echo "Check the setup:  careertwin-prep --doctor"
echo "Run:              careertwin-prep   (or open 'CareerTwin Prep' from your launcher)"
