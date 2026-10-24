"""Tell a running interview window what to do from outside: a compositor key, a script.

`careertwin-interview-ctl screenshot` answers what is on the screen. The window itself only sees
keys while it has the focus, and during a call the focus is on the interview page; a key bound
in the compositor reaches it through this small channel. (Windows has the key built in.) Neither
side needs Qt, so the command starts at once.
"""

from __future__ import annotations

import argparse
import contextlib
import os
import socket
import sys
import tempfile
import threading
from collections.abc import Callable

COMMANDS = ("screenshot", "ask", "ping")
NAME = "careertwin-interview"


class ControlError(RuntimeError):
    """The window did not answer."""


def socket_path() -> str:
    """Where the window listens: a socket in the runtime directory that only this user can open."""
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if runtime:
        return os.path.join(runtime, f"{NAME}.sock")
    return os.path.join(tempfile.gettempdir(), f"{NAME}-{os.getuid()}.sock")


def send(command: str, timeout: float = 2.0) -> str:
    """Send one command to the running window and return its one-line reply."""
    if command not in COMMANDS:
        raise ControlError(f"Unknown command {command!r}; use one of {', '.join(COMMANDS)}")
    if sys.platform == "win32":
        raise ControlError("not needed on Windows: Ctrl+Shift+Enter works in every program")
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(timeout)
            client.connect(socket_path())
            client.sendall(f"{command}\n".encode())
            return client.makefile("rb").readline().decode(errors="replace").strip()
    except OSError as exc:
        raise ControlError("CareerTwin Interview is not running") from exc


class ControlServer:
    """The window's side: a thread that reads one-word commands and hands them to `callback`."""

    def __init__(self, callback: Callable[[str], None]) -> None:
        self.callback = callback
        self.error = ""
        self._socket: socket.socket | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> bool:
        """Listen; False (with `error` set) when another window already holds the socket."""
        if sys.platform == "win32":
            return True  # the global hotkey does this job there
        with contextlib.suppress(ControlError):
            send("ping", timeout=0.5)
            self.error = "another CareerTwin Interview window is already running"
            return False
        path = socket_path()
        with contextlib.suppress(FileNotFoundError):
            os.unlink(path)  # nobody answers, so a file left here is from a crashed run
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            listener.bind(path)
            os.chmod(path, 0o600)
            listener.listen(4)
        except OSError as exc:
            listener.close()
            self.error = str(exc)
            return False
        self._socket = listener
        self._thread = threading.Thread(target=self._serve, name="control", daemon=True)
        self._thread.start()
        return True

    def stop(self) -> None:
        if self._socket is not None:
            self._socket.close()
            self._socket = None
            with contextlib.suppress(OSError):
                os.unlink(socket_path())

    def _serve(self) -> None:
        while (listener := self._socket) is not None:
            try:
                client, _ = listener.accept()
            except OSError:
                return
            with client:
                client.settimeout(1.0)
                try:
                    name = client.makefile("rb").readline().decode(errors="replace").strip()
                    if name in COMMANDS:
                        if name != "ping":
                            self.callback(name)
                        client.sendall(b"ok\n")
                    else:
                        client.sendall(f"unknown command {name!r}\n".encode())
                except OSError:
                    continue


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(
        prog="careertwin-interview-ctl",
        description="Tell the running interview window to answer the screen or the last question.",
    )
    parser.add_argument("command", choices=COMMANDS, help="screenshot: solve what is on the screen")
    args = parser.parse_args(argv)
    try:
        reply = send(args.command)
    except ControlError as exc:
        raise SystemExit(f"error: {exc}") from exc
    if reply != "ok":
        raise SystemExit(f"error: {reply or 'no answer'}")


if __name__ == "__main__":
    main()
