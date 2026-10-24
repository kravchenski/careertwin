#!/usr/bin/env python3
"""Render packaging/careertwin-interview.svg to a 256 px PNG for launchers without SVG support.

Usage: uv run --extra interview python scripts/render_icon.py
"""

from __future__ import annotations

import os
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtGui import QGuiApplication, QImage, QPainter  # noqa: E402
from PySide6.QtSvg import QSvgRenderer  # noqa: E402

PACKAGING = Path(__file__).resolve().parents[1] / "packaging"


def main(size: int = 256) -> None:
    app = QGuiApplication([])
    renderer = QSvgRenderer(str(PACKAGING / "careertwin-interview.svg"))
    image = QImage(size, size, QImage.Format.Format_ARGB32)
    image.fill(0)
    painter = QPainter(image)
    renderer.render(painter)
    painter.end()
    target = PACKAGING / "careertwin-interview.png"
    image.save(str(target))
    print(f"saved {target}")
    del app


if __name__ == "__main__":
    main()
