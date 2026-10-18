"""Talking to a running LID: one abstract-namespace Unix socket per user.

Each connection carries newline-separated lines. A plain line is an address to download
(or ``--show``); a line starting with ``{`` is a JSON request from the browser extension.
Standard library only, so the native messaging host starts fast and never loads Qt.
"""

from __future__ import annotations

import json
import os
import socket

# LIDER_SOCKET_NAME picks another name, e.g. for tests next to a running LID.
SOCKET_NAME = "\0" + (os.environ.get("LIDER_SOCKET_NAME") or f"lider-{os.getuid()}")


def send_lines(lines: list[str], name: str = SOCKET_NAME) -> bool:
    """Hand lines to the running LID. False when none is running."""
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        try:
            s.connect(name)
            s.sendall(("\n".join(lines) if lines else "--show").encode() + b"\n")
        except OSError:
            return False
    return True


def send_request(request: dict, name: str = SOCKET_NAME) -> bool:
    return send_lines([json.dumps(request, separators=(",", ":"))], name)


def is_running(name: str = SOCKET_NAME) -> bool:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        try:
            s.connect(name)
        except OSError:
            return False
    return True
