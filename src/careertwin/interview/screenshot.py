"""Capture the screen for coding tasks and slides the interviewer shares."""

from __future__ import annotations

import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

JPEG_QUALITY = 82


class ScreenshotError(RuntimeError):
    """No working screen capture tool, or it failed."""


@dataclass(frozen=True)
class Shot:
    """A captured screen: JPEG when it could be made, since it is ten times smaller than PNG."""

    data: bytes
    mime: str = "image/jpeg"

    def data_url(self) -> str:
        return f"data:{self.mime};base64,{base64.b64encode(self.data).decode()}"


# Wayland-capable tools first; ImageMagick's `import` only sees X11 and XWayland windows.
TOOLS: list[tuple[str, list[str], bool]] = [
    ("grim", ["grim", "{path}"], True),
    ("spectacle", ["spectacle", "--background", "--nonotify", "--fullscreen", "--output", "{path}"], True),
    ("gnome-screenshot", ["gnome-screenshot", "--file", "{path}"], True),
    ("maim", ["maim", "{path}"], False),
    ("scrot", ["scrot", "{path}"], False),
    ("screencapture", ["screencapture", "-x", "{path}"], False),  # built into macOS
]


# Windows captures in-process through GDI: the primary screen, or all of them side by side.
WINDOWS_TOOLS = ("primary-screen", "all-screens")


def _windows() -> bool:
    return sys.platform == "win32"


def available_tools() -> list[str]:
    """Installed capture tools that can see this session's screen, best first."""
    if _windows():
        return list(WINDOWS_TOOLS)
    wayland = bool(os.getenv("WAYLAND_DISPLAY"))
    return [
        name
        for name, _command, wayland_ok in TOOLS
        if shutil.which(name) and (wayland_ok or not wayland)
    ]


def available_tool() -> str | None:
    tools = available_tools()
    return tools[0] if tools else None


def focused_output() -> str | None:
    """The monitor that has the keyboard focus, when the compositor can say (niri).

    A task shown on one monitor is then captured alone: half the pixels, so a faster answer.
    """
    if _windows() or not os.getenv("NIRI_SOCKET") or not shutil.which("niri"):
        return None
    try:
        result = subprocess.run(
            ["niri", "msg", "--json", "focused-output"], capture_output=True, text=True, timeout=1
        )
        data = json.loads(result.stdout)
    except (OSError, subprocess.SubprocessError, ValueError):
        return None
    name = data.get("name") if isinstance(data, dict) else None
    return name if isinstance(name, str) and name else None


def capture_png(tool: str = "auto", output: str | None = None) -> bytes:
    """Return the screen (or one `output`, where the tool can) as PNG bytes using `tool`."""
    installed = available_tools()
    if _windows():
        if tool != "auto" and tool not in installed:
            raise ScreenshotError(f"{tool} is not a Windows capture mode: {', '.join(installed)}")
        from careertwin.interview import screenshot_windows

        try:
            return screenshot_windows.capture_png(all_screens=(tool == "all-screens"))
        except screenshot_windows.CaptureError as exc:
            raise ScreenshotError(str(exc)) from exc
    chosen = _choose(tool, installed)
    command = next(command for name, command, _ in TOOLS if name == chosen)
    if output and chosen == "grim":
        command = ["grim", "-o", output, "{path}"]
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


def capture(tool: str = "auto", output: str | None = None) -> Shot:
    """The screen as a Shot, as fast as the tool allows: grim writes a JPEG in about 30 ms."""
    installed = available_tools()
    if _windows():
        return _capture_windows(tool, installed)
    if _choose(tool, installed) == "grim":
        command = ["grim", "-t", "jpeg", "-q", str(JPEG_QUALITY)]
        if output:
            command += ["-o", output]
        result = subprocess.run([*command, "-"], capture_output=True, timeout=20)
        if result.returncode == 0 and result.stdout[:2] == b"\xff\xd8":
            return Shot(result.stdout)
        # An older grim without JPEG support: fall through to PNG.
    return _compress(capture_png(tool, output))


def _choose(tool: str, installed: list[str]) -> str:
    if not installed:
        raise ScreenshotError(
            "No screenshot tool. Install grim (Wayland, e.g. niri or sway) or spectacle, "
            "or maim/scrot on X11."
        )
    if tool != "auto" and tool not in installed:
        raise ScreenshotError(f"{tool} is not installed or cannot capture this session")
    return installed[0] if tool == "auto" else tool


def _capture_windows(tool: str, installed: list[str]) -> Shot:
    if tool != "auto" and tool not in installed:
        raise ScreenshotError(f"{tool} is not a Windows capture mode: {', '.join(installed)}")
    from careertwin.interview import screenshot_windows

    try:
        pixels = screenshot_windows.capture_pixels(all_screens=(tool == "all-screens"))
    except screenshot_windows.CaptureError as exc:
        raise ScreenshotError(str(exc)) from exc
    shot = _jpeg_from_bgra(pixels)
    if shot is not None:
        return shot
    return Shot(screenshot_windows.encode_png(screenshot_windows.bgra_to_rgb(pixels)), "image/png")


def _compress(png: bytes) -> Shot:
    """PNG to JPEG through Qt, which the interview window always has; PNG if that is not possible."""
    try:
        from PySide6.QtCore import QBuffer, QIODevice
        from PySide6.QtGui import QImage
    except ImportError:
        return Shot(png, "image/png")
    image = QImage.fromData(png)
    if image.isNull():
        return Shot(png, "image/png")
    buffer = QBuffer()
    buffer.open(QIODevice.OpenModeFlag.WriteOnly)
    image.convertToFormat(QImage.Format.Format_RGB32).save(buffer, "JPEG", JPEG_QUALITY)
    return Shot(bytes(buffer.data()))


def _jpeg_from_bgra(pixels) -> Shot | None:
    """Top-down BGRA rows (height, width, 4) to JPEG: the bytes are Qt's own RGB32 layout."""
    try:
        from PySide6.QtCore import QBuffer, QIODevice
        from PySide6.QtGui import QImage
    except ImportError:
        return None
    height, width, _ = pixels.shape
    data = pixels.tobytes()  # QImage does not copy it: keep it alive until the image is saved
    image = QImage(data, width, height, width * 4, QImage.Format.Format_RGB32)
    buffer = QBuffer()
    buffer.open(QIODevice.OpenModeFlag.WriteOnly)
    image.save(buffer, "JPEG", JPEG_QUALITY)
    return Shot(bytes(buffer.data()))


def to_base64(png: bytes) -> str:
    return base64.b64encode(png).decode()
