"""Capture the screen for coding tasks and slides the interviewer shares."""

from __future__ import annotations

import base64
import shutil
import subprocess
import tempfile
from pathlib import Path


class ScreenshotError(RuntimeError):
    """No working screen capture tool, or it failed."""


# Wayland-capable tools first; ImageMagick's `import` only sees X11 and XWayland windows.
TOOLS: list[tuple[str, list[str], bool]] = [
    ("grim", ["grim", "{path}"], True),
    ("spectacle", ["spectacle", "--background", "--nonotify", "--fullscreen", "--output", "{path}"], True),
    ("gnome-screenshot", ["gnome-screenshot", "--file", "{path}"], True),
    ("maim", ["maim", "{path}"], False),
    ("scrot", ["scrot", "{path}"], False),
]


def available_tools() -> list[str]:
    """Installed capture tools that can see this session's screen, best first."""
    import os

    wayland = bool(os.getenv("WAYLAND_DISPLAY"))
    return [
        name
        for name, _command, wayland_ok in TOOLS
        if shutil.which(name) and (wayland_ok or not wayland)
    ]


def available_tool() -> str | None:
    tools = available_tools()
    return tools[0] if tools else None


def capture_png(tool: str = "auto") -> bytes:
    """Return the whole screen as PNG bytes using `tool` (or the best available one)."""
    installed = available_tools()
    if not installed:
        raise ScreenshotError(
            "No screenshot tool. Install grim (Wayland, e.g. niri or sway) or spectacle, "
            "or maim/scrot on X11."
        )
    if tool != "auto" and tool not in installed:
        raise ScreenshotError(f"{tool} is not installed or cannot capture this session")
    chosen = installed[0] if tool == "auto" else tool
    command = next(command for name, command, _ in TOOLS if name == chosen)
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "screen.png"
        result = subprocess.run(
            [part.format(path=path) for part in command], capture_output=True, text=True, timeout=20
        )
        if result.returncode != 0 or not path.is_file() or path.stat().st_size == 0:
            raise ScreenshotError(
                f"{chosen} failed: {result.stderr.strip()[:200] or 'no image written'}"
            )
        return path.read_bytes()


def to_base64(png: bytes) -> str:
    return base64.b64encode(png).decode()
