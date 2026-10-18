"""Application start-up: single instance, settings, manager and main window."""

from __future__ import annotations

import argparse
import os
import socket
import sys
from typing import Callable

from PySide6.QtCore import QSocketNotifier, QTimer
from PySide6.QtWidgets import QApplication

from . import APP_LONG_NAME, APP_NAME
from .core import settings as settings_mod
from .core.manager import DownloadManager
from .core.store import Store

# Abstract-namespace Unix socket: no file is left behind if LID crashes. Plain sockets
# keep QtNetwork (and its Kerberos dependency) out of the process.
SOCKET_NAME = f"\0lider-{os.getuid()}"


def send_to_running(lines: list[str], name: str = SOCKET_NAME) -> bool:
    """Hand the URLs to an already running LID. False when none is running."""
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        try:
            s.connect(name)
            s.sendall(("\n".join(lines) if lines else "--show").encode() + b"\n")
        except OSError:
            return False
    return True


class InstanceServer:
    """Receives the lines sent by ``send_to_running`` on the Qt event loop."""

    def __init__(self, on_line: Callable[[str], None], name: str = SOCKET_NAME):
        self.on_line = on_line
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.bind(name)
        self.sock.listen(8)
        self.sock.setblocking(False)
        self.notifier = QSocketNotifier(self.sock.fileno(), QSocketNotifier.Read)
        self.notifier.activated.connect(self._accept)

    def _accept(self, *_):
        try:
            conn, _ = self.sock.accept()
        except BlockingIOError:
            return
        with conn:
            conn.settimeout(2)
            data = b""
            try:
                while chunk := conn.recv(65536):
                    data += chunk
            except OSError:
                pass
        for line in data.decode(errors="replace").splitlines():
            if line.strip():
                self.on_line(line.strip())

    def close(self):
        self.notifier.setEnabled(False)
        self.sock.close()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="lider", description=f"{APP_NAME}: {APP_LONG_NAME}")
    parser.add_argument("urls", nargs="*", help="addresses to download")
    parser.add_argument("--minimized", action="store_true", help="start hidden in the system tray")
    args, qt_args = parser.parse_known_args(argv if argv is not None else sys.argv[1:])

    if send_to_running(args.urls):
        return 0

    app = QApplication([sys.argv[0], *qt_args])
    app.setApplicationName(APP_NAME)
    app.setApplicationDisplayName(APP_NAME)
    app.setDesktopFileName("lider")
    app.setQuitOnLastWindowClosed(False)

    from .ui.icons import icon
    from .ui.main_window import MainWindow

    app.setWindowIcon(icon("app"))

    settings_mod.migrate_legacy_dirs()
    settings = settings_mod.load()
    store = Store()
    manager = DownloadManager(settings, store)
    win = MainWindow(manager)

    def on_line(line: str) -> None:
        win.show_normal()
        if line != "--show":
            win.add_url(line, ask_address=False)

    try:
        server = InstanceServer(on_line)
    except OSError:
        # Another LID started at the same moment and owns the socket.
        if send_to_running(args.urls):
            return 0
        server = None

    if not (args.minimized and win.tray):
        win.show()
    for url in args.urls:
        QTimer.singleShot(0, lambda u=url: win.add_url(u, ask_address=False))
    rc = app.exec()
    if server:
        server.close()
    store.close()
    return rc
