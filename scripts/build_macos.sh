#!/usr/bin/env bash
# Build "CareerTwin Interview.app" and a dmg from it. Run it on macOS (the wheels are for the Mac
# that builds, and hdiutil makes the dmg). Output: dist/macos/CareerTwin-Interview-<version>-macos-<arch>.dmg
#
# The app carries its own Python (the standalone build uv installs) and the packages the window
# imports, so nothing needs installing. It is signed ad hoc, not by Apple: the first start needs
# right-click -> Open (see README.txt in the dmg).
#
# SKIP_NATIVE=1 stops before the steps only macOS can do (icon, signing, dmg), to check the bundle
# on another system.
set -euo pipefail

cd "$(dirname "$0")/.."
PYTHON_MINOR="${PYTHON_MINOR:-3.14}"
NAME="CareerTwin Interview"
OUT=dist/macos
APP="$OUT/$NAME.app"
RES="$APP/Contents/Resources"
VERSION="$(grep -m1 '^version = ' pyproject.toml | cut -d'"' -f2)"
ARCH="$(uname -m)"

rm -rf "$OUT" dist/wheel
mkdir -p "$APP/Contents/MacOS" "$RES" dist/wheel

export UV_PYTHON_PREFERENCE=only-managed  # a relocatable build, never the system or Homebrew Python
uv python install "$PYTHON_MINOR"
PYTHON="$(uv python find "$PYTHON_MINOR")"
PREFIX="$("$PYTHON" -c 'import sys; print(sys.prefix)')"
cp -R "$PREFIX" "$RES/python"
rm -rf "$RES"/python/lib/python*/{test,idlelib,tkinter,turtledemo,ensurepip} "$RES"/python/lib/python*/lib2to3 2>/dev/null || true
BUNDLED="$RES/python/bin/python3"

uv build --wheel --out-dir dist/wheel >/dev/null
uv pip install --quiet --no-deps --break-system-packages --python "$BUNDLED" \
    --target "$RES/site-packages" dist/wheel/careertwin-*.whl
# Only what the interview window imports: the API, agents and job pipeline stay out of the app.
uv pip install --quiet --break-system-packages --python "$BUNDLED" --target "$RES/site-packages" \
    faster-whisper numpy pyside6-essentials httpx python-dotenv pypdf python-docx
find "$RES/site-packages" -name '__pycache__' -type d -prune -exec rm -rf {} +

cat > "$APP/Contents/MacOS/careertwin-interview" <<'LAUNCHER'
#!/bin/sh
# Starts the interview window with the Python inside this app. Settings and documents live in
# Application Support, so replacing the app keeps them.
contents="$(cd "$(dirname "$0")/.." && pwd)"
export PYTHONPATH="$contents/Resources/site-packages"
export PYTHONNOUSERSITE=1
export CAREERTWIN_DATA_DIR="${CAREERTWIN_DATA_DIR:-$HOME/Library/Application Support/CareerTwin Interview}"
mkdir -p "$CAREERTWIN_DATA_DIR"
cd "$CAREERTWIN_DATA_DIR" || exit 1  # a .env placed here is read too
exec "$contents/Resources/python/bin/python3" -m careertwin.interview.app "$@" >> "$CAREERTWIN_DATA_DIR/last-run.log" 2>&1
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
        "$OUT/CareerTwin-Interview-$VERSION-macos-$ARCH.dmg" >/dev/null; then
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
