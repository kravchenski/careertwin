"""Ctrl+Shift+Enter anywhere in Windows, even while another program has the focus.

`RegisterHotKey` belongs to the thread that calls it, so that thread also runs the message loop
and hands each press to a callback.
"""

from __future__ import annotations

import ctypes
import threading
from collections.abc import Callable
from ctypes import wintypes

MOD_CONTROL, MOD_SHIFT, MOD_NOREPEAT = 0x0002, 0x0004, 0x4000
VK_RETURN = 0x0D
WM_HOTKEY, WM_QUIT = 0x0312, 0x0012
HOTKEY_ID = 0x4354  # "CT"


class GlobalHotkey:
    """Runs `callback` on a helper thread for every press; `libraries` is injectable for tests."""

    def __init__(
        self,
        callback: Callable[[], None],
        modifiers: int = MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
        key: int = VK_RETURN,
        libraries: tuple | None = None,
    ) -> None:
        self.callback = callback
        self.modifiers, self.key = modifiers, key
        self._libraries = libraries
        self.registered = False
        self._thread: threading.Thread | None = None
        self._thread_id = 0
        self._ready = threading.Event()

    def start(self) -> bool:
        """Register the key; False when another program holds it."""
        self._thread = threading.Thread(target=self._run, name="hotkey", daemon=True)
        self._thread.start()
        self._ready.wait(3)
        return self.registered

    def stop(self) -> None:
        if self._thread is not None and self._thread_id:
            user32 = self._load()[0]
            user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0)
            self._thread.join(1)

    def _load(self) -> tuple:
        if self._libraries is not None:
            return self._libraries
        return ctypes.WinDLL("user32", use_last_error=True), ctypes.WinDLL("kernel32", use_last_error=True)

    def _run(self) -> None:
        user32, kernel32 = self._load()
        self._thread_id = kernel32.GetCurrentThreadId()
        self.registered = bool(user32.RegisterHotKey(None, HOTKEY_ID, self.modifiers, self.key))
        self._ready.set()
        if not self.registered:
            return
        message = wintypes.MSG()
        while user32.GetMessageW(ctypes.byref(message), None, 0, 0) > 0:
            if message.message == WM_HOTKEY:
                self.callback()
        user32.UnregisterHotKey(None, HOTKEY_ID)
