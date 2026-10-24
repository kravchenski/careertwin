#!/usr/bin/env bash
# Build the portable Windows (x64) package of the interview assistant from Linux or macOS.
#
# No Windows machine or Wine is needed to build: uv fetches the Windows wheels, and the program
# runs on the python.org embeddable Python. Output: dist/windows/CareerTwin-Interview-windows-x64.zip
set -euo pipefail

cd "$(dirname "$0")/.."
PYTHON_VERSION="${PYTHON_VERSION:-3.14.9}"
NAME=CareerTwin-Interview
OUT=dist/windows
APP="$OUT/$NAME"
CACHE=dist/cache
EMBED="python-$PYTHON_VERSION-embed-amd64.zip"

rm -rf "$APP" "$OUT/$NAME-windows-x64.zip" dist/wheel
mkdir -p "$APP/python" "$APP/site-packages" "$CACHE" dist/wheel

if [ ! -f "$CACHE/$EMBED" ]; then
    curl -fL -o "$CACHE/$EMBED" "https://www.python.org/ftp/python/$PYTHON_VERSION/$EMBED"
fi
unzip -q "$CACHE/$EMBED" -d "$APP/python"
# The embeddable Python ignores PYTHONPATH and site; its search path is this file.
printf 'python%s.zip\n.\n..\\site-packages\n' "$(echo "$PYTHON_VERSION" | awk -F. '{print $1 $2}')" \
    > "$APP/python/python$(echo "$PYTHON_VERSION" | awk -F. '{print $1 $2}')._pth"

uv build --wheel --out-dir dist/wheel >/dev/null
uv pip install --quiet --no-deps --python-platform x86_64-pc-windows-msvc \
    --python-version "${PYTHON_VERSION%.*}" --target "$APP/site-packages" dist/wheel/careertwin-*.whl
# Only what the interview window imports: the API, agents and job pipeline stay out of the package.
uv pip install --quiet --python-platform x86_64-pc-windows-msvc \
    --python-version "${PYTHON_VERSION%.*}" --target "$APP/site-packages" \
    faster-whisper numpy pyside6-essentials soundcard httpx python-dotenv pypdf python-docx

cp packaging/windows/start.pyw packaging/windows/careertwin-interview.ico "$APP/"
for file in "Start CareerTwin Interview.bat" "Check setup.bat" "Create shortcut.bat" README-Windows.txt; do
    sed 's/$/\r/' "packaging/windows/$file" > "$APP/$file"   # Windows scripts want CRLF
done
find "$APP/site-packages" -name '__pycache__' -type d -prune -exec rm -rf {} +

(cd "$OUT" && zip -qr "$NAME-windows-x64.zip" "$NAME")
du -sh "$APP" "$OUT/$NAME-windows-x64.zip"
