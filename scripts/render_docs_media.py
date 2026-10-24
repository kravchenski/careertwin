#!/usr/bin/env python3
"""Rebuild the pictures the README uses from the current code, with made-up data (no model is called).

Usage: uv run --extra interview python scripts/render_docs_media.py [output folder, default docs/media]

Writes interview-overlay.png, interview-code.png, interview-settings.png, interview-project.png and
interview-languages.png. The demo video is made by scripts/demo/record.py.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
LANGUAGES = ("en", "uk", "pl", "de", "fr", "es")


def run(script: str, *args: str) -> None:
    subprocess.run([sys.executable, str(HERE / script), *args], check=True, stdout=subprocess.DEVNULL)


def montage(parts: list[Path], target: Path, columns: int = 3, width: int = 640) -> None:
    """The top of each window (the buttons, the question, the first lines) side by side."""
    tiles = []
    for part in parts:
        image = Image.open(part).convert("RGB")
        image = image.crop((0, 0, image.width, int(image.height * 0.5)))
        tiles.append(image.resize((width, int(image.height * width / image.width)), Image.LANCZOS))
    height = max(tile.height for tile in tiles)
    rows = -(-len(tiles) // columns)
    gap = 14
    sheet = Image.new("RGB", (columns * width + (columns + 1) * gap, rows * height + (rows + 1) * gap), (15, 17, 21))
    for index, tile in enumerate(tiles):
        x = gap + (index % columns) * (width + gap)
        y = gap + (index // columns) * (height + gap)
        sheet.paste(tile, (x, y))
    sheet.save(target, optimize=True)


def main(out: str = "docs/media") -> None:
    folder = Path(out)
    folder.mkdir(parents=True, exist_ok=True)
    run("render_overlay_preview.py", str(folder / "interview-overlay.png"), "en", "answer")
    run("render_overlay_preview.py", str(folder / "interview-code.png"), "en", "code")
    run("render_settings_preview.py", str(folder / "interview-settings.png"), "interview", "en")
    run("render_settings_preview.py", str(folder / "interview-project.png"), "project", "en")
    with tempfile.TemporaryDirectory() as work:
        parts = []
        for code in LANGUAGES:
            part = Path(work) / f"{code}.png"
            run("render_overlay_preview.py", str(part), code, "answer")
            parts.append(part)
        montage(parts, folder / "interview-languages.png")
    for name in sorted(folder.glob("interview-*.png")):
        print(f"{name}: {name.stat().st_size / 1024:.0f} KB")


if __name__ == "__main__":
    main(*sys.argv[1:2])
