#!/usr/bin/env bash
# Build the Rust interview window and pack it for Linux or macOS. Run it on that system.
#
#   scripts/package_overlay.sh linux  [label]   ->  dist/linux/CareerTwin-Interview-<label>-linux-<arch>.tar.gz
#   scripts/package_overlay.sh macos  [label]   ->  dist/macos/CareerTwin-Interview-<label>-macos-<arch>.dmg
#
# <label> is a version such as v0.6.0 (default: the version in overlay/Cargo.toml). Windows is packed by the
# release workflow itself (PowerShell and Inno Setup).
#
# The Mac app is signed ad hoc, not by Apple: the first start needs right-click -> Open (README.txt in the dmg).
# SKIP_BUILD=1 packs a window that was built already; SKIP_NATIVE=1 stops the macOS packing before the icon,
# signing and dmg, which only macOS can do.
set -euo pipefail

cd "$(dirname "$0")/.."
KIND="${1:?usage: package_overlay.sh linux|macos [label]}"
VERSION="$(grep -m1 '^version = ' overlay/Cargo.toml | cut -d'"' -f2)"
LABEL="${2:-v$VERSION}"
ARCH="$(uname -m)"
NAME="CareerTwin Interview"

if [ "${SKIP_BUILD:-}" != 1 ]; then
    cargo build --release --locked --manifest-path overlay/Cargo.toml
fi
TARGET="$(cargo metadata --manifest-path overlay/Cargo.toml --format-version 1 --no-deps |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
BIN="$TARGET/release/careertwin-overlay"
[ -x "$BIN" ] || { echo "no window was built at $BIN" >&2; exit 1; }

case "$KIND" in
linux)
    OUT=dist/linux
    DIR="$OUT/CareerTwin-Interview"
    rm -rf "$OUT"
    mkdir -p "$DIR"
    cp "$BIN" "$DIR/careertwin-overlay"
    strip "$DIR/careertwin-overlay" 2>/dev/null || true
    cat > "$DIR/careertwin-interview-ctl" <<'CTL'
#!/bin/sh
# Tell the running window what to do: bind `careertwin-interview-ctl screenshot` to a key in your compositor.
exec "$(dirname "$(readlink -f "$0")")/careertwin-overlay" ctl "$@"
CTL
    chmod +x "$DIR/careertwin-interview-ctl"
    cp packaging/careertwin-interview.desktop packaging/careertwin-interview.png "$DIR/"
    cat > "$DIR/install.sh" <<'INSTALL'
#!/bin/sh
# Copy the window into ~/.local (no administrator rights) and add it to the application menu.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
bin="${PREFIX:-$HOME/.local}/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons"
mkdir -p "$bin" "$apps" "$icons"
install -m 755 "$here/careertwin-overlay" "$bin/careertwin-overlay"
install -m 755 "$here/careertwin-interview-ctl" "$bin/careertwin-interview-ctl"
ln -sf careertwin-overlay "$bin/careertwin-interview"
install -m 644 "$here/careertwin-interview.png" "$icons/careertwin-interview.png"
sed -e "s#@BIN@#$bin#" -e "s#@ICON@#$icons/careertwin-interview.png#" "$here/careertwin-interview.desktop" > "$apps/careertwin-interview.desktop"
echo "Installed into $bin. Start it from the menu or with: careertwin-interview"
INSTALL
    chmod +x "$DIR/install.sh"
    cat > "$DIR/README.txt" <<README
CareerTwin Interview $LABEL for Linux ($ARCH)

Run ./install.sh to copy it into ~/.local and add it to the application menu, or start ./careertwin-overlay
from this folder. Your settings and documents are kept in ~/.local/share/careertwin (or the folder named by
CAREERTWIN_DATA_DIR).

Hearing the call needs parec and pactl (pipewire-pulse or pulseaudio-utils). A screenshot needs grim on Wayland
(niri, sway), or spectacle, gnome-screenshot, maim or scrot. On Wayland bind a key in your compositor to
careertwin-interview-ctl screenshot; the window must be running.
README
    tar -C "$OUT" -czf "$OUT/CareerTwin-Interview-$LABEL-linux-$ARCH.tar.gz" CareerTwin-Interview
    rm -rf "$DIR"
    ls -lh "$OUT"
    ;;
macos)
    OUT=dist/macos
    APP="$OUT/$NAME.app"
    RES="$APP/Contents/Resources"
    rm -rf "$OUT"
    mkdir -p "$APP/Contents/MacOS" "$RES"
    cp "$BIN" "$APP/Contents/MacOS/careertwin-overlay"
    strip "$APP/Contents/MacOS/careertwin-overlay" 2>/dev/null || true
    cat > "$APP/Contents/MacOS/careertwin-interview" <<'LAUNCHER'
#!/bin/sh
# Starts the window. Settings and documents live in Application Support, so replacing the app keeps them.
contents="$(cd "$(dirname "$0")/.." && pwd)"
export CAREERTWIN_DATA_DIR="${CAREERTWIN_DATA_DIR:-$HOME/Library/Application Support/CareerTwin Interview}"
mkdir -p "$CAREERTWIN_DATA_DIR"
exec "$contents/MacOS/careertwin-overlay" "$@"
LAUNCHER
    chmod +x "$APP/Contents/MacOS/careertwin-interview"
    cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$NAME</string>
    <key>CFBundleDisplayName</key><string>$NAME</string>
    <key>CFBundleIdentifier</key><string>io.github.kravchenski.careertwin-interview</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key><string>careertwin-interview</string>
    <key>CFBundleIconFile</key><string>careertwin-interview</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSMicrophoneUsageDescription</key><string>CareerTwin Interview listens when you ask it to transcribe your microphone.</string>
</dict>
</plist>
PLIST
    if [ "${SKIP_NATIVE:-}" = 1 ]; then
        du -sh "$APP"
        exit 0
    fi
    # The icon: the PNG in packaging/ at every size macOS asks for.
    ICONSET="$OUT/icon.iconset"
    mkdir -p "$ICONSET"
    for size in 16 32 128 256 512; do
        sips -z "$size" "$size" packaging/careertwin-interview.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
        sips -z "$((size * 2))" "$((size * 2))" packaging/careertwin-interview.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$ICONSET" -o "$RES/careertwin-interview.icns"
    rm -rf "$ICONSET"
    codesign --force --deep --sign - "$APP"
    STAGE="$OUT/dmg"
    mkdir -p "$STAGE"
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"
    cp packaging/macos/README-macOS.txt "$STAGE/README.txt"
    # hdiutil fails now and then with "Resource busy" while macOS indexes the new files: try again.
    for attempt in 1 2 3 4 5; do
        if hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO \
            "$OUT/CareerTwin-Interview-$LABEL-macos-$ARCH.dmg" >/dev/null; then
            break
        fi
        if [ "$attempt" = 5 ]; then
            echo "hdiutil kept failing" >&2
            exit 1
        fi
        echo "hdiutil failed (attempt $attempt), trying again" >&2
        sleep $((attempt * 5))
    done
    rm -rf "$STAGE"
    du -sh "$APP" "$OUT"/*.dmg
    ;;
*)
    echo "usage: package_overlay.sh linux|macos [label]" >&2
    exit 2
    ;;
esac
