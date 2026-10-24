"""Hide the interview window from screen captures while it stays on your monitor.

Windows DWM (10 version 2004+, and 11) drops a window from every capture surface it
services — Zoom, Teams, Meet, OBS, PrintWindow — when the window carries
`SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, while still rendering it normally on
the physical display. Older builds accept the flag but fall back to `WDA_MONITOR`, which
blacks the window out in captures instead of erasing it. Windows may silently reset the
affinity on some window-manager events, so callers re-apply it periodically.

Other platforms have no equivalent: on Wayland the only options are a black rectangle
(niri `block-out-from`) or hiding the window from both sides. Keep the window on a
monitor you do not share there.
"""

from __future__ import annotations

import sys

WDA_EXCLUDEFROMCAPTURE = 0x00000011


def is_supported() -> bool:
    """True where the compositor can honour the exclusion."""
    return sys.platform == "win32"


def exclude(hwnd: int, user32=None) -> bool:
    """Ask Windows never to include this window in a capture; False off Windows.

    `user32` is injectable for tests; the real caller is `ctypes.windll.user32`.
    """
    if user32 is None:
        if sys.platform != "win32":
            return False
        import ctypes

        user32 = ctypes.windll.user32
    return bool(user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE))
