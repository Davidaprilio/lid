"""Application start-up: single instance, settings, manager and main window."""

from __future__ import annotations

import argparse
import json
import socket
import sys
from typing import Callable

from PySide6.QtCore import QSocketNotifier, QTimer
from PySide6.QtWidgets import QApplication

from . import APP_LONG_NAME, APP_NAME
from .ipc import SOCKET_NAME, send_lines
from .core import settings as settings_mod
from .core.manager import DownloadManager
from .core.store import Store

# Plain sockets (see ipc.py) keep QtNetwork and its Kerberos dependency out of the process.
send_to_running = send_lines


class InstanceServer:
    """Receives the lines sent through ipc.py on the Qt event loop."""

    def __init__(self, on_line: Callable[[str], None], name: str = SOCKET_NAME):
        self.on_line = on_line
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            self.sock.bind(name)
        except OSError:
            self.sock.close()
            raise
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
    raw = argv if argv is not None else sys.argv[1:]
    if "--native-host" in raw:
        from .native_host import main as native_main

        return native_main()
    if "--install-browser-integration" in raw or "--uninstall-browser-integration" in raw:
        from .browser import main as browser_main

        return browser_main(raw)
    parser = argparse.ArgumentParser(prog="lider", description=f"{APP_NAME}: {APP_LONG_NAME}")
    parser.add_argument("urls", nargs="*", help="addresses to download")
    parser.add_argument("--minimized", action="store_true", help="start hidden in the system tray")
    args, qt_args = parser.parse_known_args(raw)

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
        if line.startswith("{"):
            try:
                request = json.loads(line)
            except ValueError:
                return
            if isinstance(request, dict):
                win.handle_browser_request(request)
            return
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
