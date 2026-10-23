"""Windows keeps the window out of captures; other platforms fall back to geometry."""

from __future__ import annotations

import sys

from careertwin.interview import capture_exclude


class FakeUser32:
    def __init__(self, result: bool = True):
        self.calls: list[tuple[int, int]] = []
        self.result = result

    def SetWindowDisplayAffinity(self, hwnd: int, affinity: int) -> bool:  # noqa: N802
        self.calls.append((hwnd, affinity))
        return self.result


def test_exclude_sends_the_capture_flag():
    user32 = FakeUser32()
    assert capture_exclude.exclude(42, user32)
    assert user32.calls == [(42, capture_exclude.WDA_EXCLUDEFROMCAPTURE)]


def test_exclude_reports_a_rejected_call():
    assert not capture_exclude.exclude(42, FakeUser32(result=False))


def test_is_supported_follows_the_platform():
    assert capture_exclude.is_supported() is (sys.platform == "win32")


def test_exclude_without_user32_is_a_noop_off_windows(monkeypatch):
    monkeypatch.setattr(sys, "platform", "linux")
    assert capture_exclude.exclude(1) is False
