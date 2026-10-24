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
import stat
import sys
import tempfile
import threading
from collections.abc import Callable

COMMANDS = ("screenshot", "ask", "ping")
NAME = "careertwin-interview"


class ControlError(RuntimeError):
    """The window did not answer."""


def _private(path: str) -> bool:
    """A real directory (not a link) that belongs to this user and nobody else can enter."""
    info = os.lstat(path)
    return stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid() and not info.st_mode & 0o077


def socket_path() -> str:
    """Where the window listens: a socket inside a directory that only this user can enter.

    That is the runtime directory when it is private, else `<tmp>/careertwin-interview-<uid>/`, created
    with mode 0700 and checked (a directory someone else made there, or a link, is refused): a socket in the
    shared /tmp could otherwise be connected to by another account between `bind` and `chmod`."""
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if runtime and os.path.isdir(runtime) and _private(runtime):
        return os.path.join(runtime, f"{NAME}.sock")
    folder = os.path.join(tempfile.gettempdir(), f"{NAME}-{os.getuid()}")
    try:
        os.mkdir(folder, 0o700)
    except FileExistsError:
        pass
    except OSError as exc:
        raise ControlError(f"cannot create {folder}: {exc}") from exc
    if not _private(folder):
        raise ControlError(f"{folder} is not private to you; remove it and start the window again")
    return os.path.join(folder, f"{NAME}.sock")


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
        try:
            path = socket_path()
        except ControlError as exc:
            self.error = str(exc)
            return False
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
