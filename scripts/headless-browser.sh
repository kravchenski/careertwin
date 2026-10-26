#!/usr/bin/env bash
# Chromium without a screen, with the headless build of Switchyard's extension, so the form filler has a browser on a
# server. Build the extension first: see docs/HOSTING.md, step 8.
#
#   scripts/headless-browser.sh ~/switchyard-extension/dist-headless
#
# PROFILE sets the browser profile (default ~/.config/careertwin-browser): keep it empty of anything of yours.
# BROWSER names the program; CHROMIUM_FLAGS adds flags (--no-sandbox only inside a container as root).
set -euo pipefail

extension=${1:?usage: headless-browser.sh <the dist-headless folder of the extension>}
[[ -f "$extension/manifest.json" ]] || { echo "No manifest.json in $extension: build the extension with 'bun run build --headless'" >&2; exit 1; }
browser=${BROWSER:-$(command -v chromium || command -v chromium-browser || command -v google-chrome || true)}
[[ -n "$browser" ]] || { echo "No chromium, chromium-browser or google-chrome on this machine" >&2; exit 1; }
profile=${PROFILE:-$HOME/.config/careertwin-browser}
mkdir -p "$profile" && chmod 700 "$profile"

# shellcheck disable=SC2086  # CHROMIUM_FLAGS is a list of flags
exec "$browser" --headless=new --no-first-run --no-default-browser-check \
  --user-data-dir="$profile" --disable-extensions-except="$extension" --load-extension="$extension" \
  --window-size=1366,900 ${CHROMIUM_FLAGS:-} about:blank
