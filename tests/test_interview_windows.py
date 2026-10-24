"""On Windows the call is not captured yet; the app says so instead of asking for pipewire-pulse."""

from __future__ import annotations

import pytest

from careertwin.interview import audio, screenshot


@pytest.fixture
def windows(monkeypatch):
    monkeypatch.setattr("sys.platform", "win32")


def test_pactl_reports_that_windows_cannot_listen_yet(windows):
    with pytest.raises(audio.AudioError, match="not available on Windows"):
        audio.default_sources()


def test_a_source_reports_that_windows_cannot_listen_yet(windows):
    with pytest.raises(audio.AudioError, match="not available on Windows"):
        next(audio.ParecSource("any").frames())


def test_screenshots_report_that_windows_has_no_tool_yet(windows, monkeypatch):
    monkeypatch.setattr(screenshot, "available_tools", lambda: [])
    with pytest.raises(screenshot.ScreenshotError, match="Windows"):
        screenshot.capture_png()
