"""Native messaging host for the LID browser extension.

The browser starts this program and talks to it over stdin/stdout: every message is a 32-bit
native-endian length followed by that many bytes of UTF-8 JSON. Requests are checked and handed
to the running LID (starting it when needed); the reply says whether LID took them.

Requests:
  {"type": "hello"}                      -> version, capture switch, file types, excluded sites
  {"type": "download", "url": ..., ...}  -> one address with referrer, cookies, user agent...
  {"type": "links", "items": [...], ...} -> a list of links for the "Download all links" dialog
  {"type": "show"}                       -> bring the LID window forward
"""

from __future__ import annotations

import json
import os
import re
import shlex
import struct
import subprocess
import sys
import time

from . import __version__, ipc
from .core import settings as settings_mod

MAX_MESSAGE = 8 * 1024 * 1024
URL_RE = re.compile(r"^(https?|ftp)://\S+$", re.I)
TEXT_FIELDS = {"url": 8192, "final_url": 8192, "referer": 8192, "page_url": 8192, "filename": 1024,
               "mime": 256, "cookies": 65536, "user_agent": 1024}
MAX_LINKS = 5000


def read_message(stream) -> dict | None:
    head = stream.read(4)
    if len(head) < 4:
        return None
    (size,) = struct.unpack("=I", head)
    if size > MAX_MESSAGE:
        raise ValueError(f"message of {size} bytes is too large")
    body = stream.read(size)
    if len(body) < size:
        return None
    msg = json.loads(body.decode("utf-8"))
    if not isinstance(msg, dict):
        raise ValueError("a message must be a JSON object")
    return msg


def write_message(stream, obj: dict) -> None:
    data = json.dumps(obj, separators=(",", ":")).encode("utf-8")
    stream.write(struct.pack("=I", len(data)) + data)
    stream.flush()


def _clean_download(msg: dict) -> dict | None:
    req = {"action": "download"}
    for key, limit in TEXT_FIELDS.items():
        value = msg.get(key)
        if isinstance(value, str) and value:
            req[key] = value[:limit]
    size = msg.get("size")
    if isinstance(size, int) and size > 0:
        req["size"] = size
    if not URL_RE.match(req.get("url", "")):
        return None
    for key in ("final_url", "referer", "page_url"):
        if key in req and not URL_RE.match(req[key]):
            del req[key]
    return req


def _clean_links(msg: dict) -> dict | None:
    items = []
    for item in (msg.get("items") or [])[:MAX_LINKS]:
        if not isinstance(item, dict):
            continue
        url = item.get("url")
        if isinstance(url, str) and URL_RE.match(url) and len(url) <= 8192:
            text = item.get("text") if isinstance(item.get("text"), str) else ""
            kind = item.get("kind") if item.get("kind") in ("a", "img", "video", "audio") else "a"
            items.append({"url": url, "text": text[:300], "kind": kind})
    if not items:
        return None
    req = {"action": "links", "items": items}
    for key in ("referer", "page_url", "cookies", "user_agent"):
        value = msg.get(key)
        if isinstance(value, str) and value:
            req[key] = value[:TEXT_FIELDS[key]]
    return req


def launch_command() -> list[str]:
    """How to start LID: the wrapper written by browser.py sets LIDER_LAUNCH."""
    if os.environ.get("LIDER_LAUNCH"):
        return shlex.split(os.environ["LIDER_LAUNCH"])
    return [sys.executable, "-m", "lider", "--minimized"]


def deliver(request: dict, socket_name: str = ipc.SOCKET_NAME, launch=None, wait: float = 15.0) -> bool:
    """Send to the running LID; start it and retry when it is not running."""
    if ipc.send_request(request, socket_name):
        return True
    cmd = launch if launch is not None else launch_command()
    try:
        subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
    except OSError:
        return False
    end = time.monotonic() + wait
    while time.monotonic() < end:
        time.sleep(0.25)
        if ipc.send_request(request, socket_name):
            return True
    return False


def handle(msg: dict, socket_name: str = ipc.SOCKET_NAME, launch=None) -> dict:
    kind = msg.get("type")
    if kind == "hello":
        s = settings_mod.load()
        return {"ok": True, "version": __version__, "capture": s.browser_capture,
                "types": s.auto_capture_types.split(), "skip_sites": s.no_auto_sites.split(),
                "running": ipc.is_running(socket_name)}
    if kind in ("download", "links", "show"):
        if kind == "show":
            request = {"action": "show"}
        else:
            request = _clean_download(msg) if kind == "download" else _clean_links(msg)
        if request is None:
            return {"ok": False, "error": "invalid request"}
        if deliver(request, socket_name, launch):
            return {"ok": True}
        return {"ok": False, "error": "LID is not running and could not be started"}
    return {"ok": False, "error": f"unknown message type {kind!r}"}


def main() -> int:
    stdin, stdout = sys.stdin.buffer, sys.stdout.buffer
    while True:
        try:
            msg = read_message(stdin)
        except (ValueError, UnicodeDecodeError) as e:
            write_message(stdout, {"ok": False, "error": str(e)})
            return 1
        if msg is None:
            return 0
        write_message(stdout, handle(msg))


if __name__ == "__main__":
    sys.exit(main())
