"""Screenshots on Windows: the capture modes, the dispatch, and the PNG the GDI pixels become."""

from __future__ import annotations

import struct
import zlib

import numpy as np
import pytest

from careertwin.interview import screenshot, screenshot_windows


@pytest.fixture
def windows(monkeypatch):
    monkeypatch.setattr("sys.platform", "win32")


def test_windows_offers_the_primary_screen_and_all_screens(windows):
    assert screenshot.available_tools() == ["primary-screen", "all-screens"]
    assert screenshot.available_tool() == "primary-screen"


def test_the_mode_chooses_one_screen_or_all(windows, monkeypatch):
    asked = []
    monkeypatch.setattr(screenshot_windows, "capture_png", lambda all_screens=False: asked.append(all_screens) or b"png")
    assert screenshot.capture_png("auto") == b"png"
    screenshot.capture_png("primary-screen")
    screenshot.capture_png("all-screens")
    assert asked == [False, False, True]


def test_a_linux_tool_name_is_refused_on_windows(windows):
    with pytest.raises(screenshot.ScreenshotError, match="not a Windows capture mode"):
        screenshot.capture_png("grim")


def test_a_failed_copy_becomes_a_screenshot_error(windows, monkeypatch):
    def fail(all_screens=False):
        raise screenshot_windows.CaptureError("Windows could not copy the screen (error 5)")

    monkeypatch.setattr(screenshot_windows, "capture_png", fail)
    with pytest.raises(screenshot.ScreenshotError, match="could not copy"):
        screenshot.capture_png("auto")


def test_gdi_pixels_become_rgb():
    bgra = np.array([[[10, 20, 30, 255], [1, 2, 3, 255]]], dtype=np.uint8)  # blue, green, red, unused
    assert screenshot_windows.bgra_to_rgb(bgra).tolist() == [[[30, 20, 10], [3, 2, 1]]]


def test_the_png_decodes_to_the_same_pixels():
    rgb = np.arange(2 * 3 * 3, dtype=np.uint8).reshape(2, 3, 3)
    png = screenshot_windows.encode_png(rgb)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    chunks, position = {}, 8
    while position < len(png):
        length, tag = struct.unpack(">I4s", png[position:position + 8])
        data = png[position + 8:position + 8 + length]
        assert struct.unpack(">I", png[position + 8 + length:position + 12 + length])[0] == zlib.crc32(tag + data)
        chunks[tag] = data
        position += 12 + length
    width, height, depth, colour = struct.unpack(">IIBB", chunks[b"IHDR"][:10])
    assert (width, height, depth, colour) == (3, 2, 8, 2)
    rows = np.frombuffer(zlib.decompress(chunks[b"IDAT"]), dtype=np.uint8).reshape(2, 3 * 3 + 1)
    assert rows[:, 0].tolist() == [0, 0] and rows[:, 1:].reshape(2, 3, 3).tolist() == rgb.tolist()
