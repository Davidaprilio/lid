"""Local HTTP server for engine tests: Range support, throttling and faults."""

from __future__ import annotations

import hashlib
import os
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class FileServer:
    def __init__(self, size: int = 3 * 1024 * 1024, ranges: bool = True, length: bool = True,
                 delay_per_chunk: float = 0.0, drop_after: int = 0, data: bytes | None = None,
                 fail_first: int = 0):
        self.data = data if data is not None else os.urandom(size)
        self.sha256 = hashlib.sha256(self.data).hexdigest()
        self.ranges = ranges
        self.length = length
        self.delay = delay_per_chunk
        self.drop_after = drop_after  # first response drops after N bytes
        self.requests: list[str] = []
        self.fail_first = fail_first  # answer the first N requests with 403 Forbidden
        self._dropped = False
        server = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *a):
                pass

            def do_GET(self):
                rng = self.headers.get("Range")
                server.requests.append(rng or "")
                if server.fail_first > 0:
                    server.fail_first -= 1
                    self.send_response(403)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                start, end = 0, len(server.data) - 1
                m = re.match(r"bytes=(\d+)-(\d*)", rng or "")
                partial = bool(m and server.ranges)
                if partial:
                    start = int(m.group(1))
                    if m.group(2):
                        end = int(m.group(2))
                    if start >= len(server.data):
                        self.send_response(416)
                        self.send_header("Content-Range", f"bytes */{len(server.data)}")
                        self.send_header("Content-Length", "0")
                        self.end_headers()
                        return
                body = server.data[start:end + 1]
                self.send_response(206 if partial else 200)
                if partial:
                    self.send_header("Content-Range", f"bytes {start}-{end}/{len(server.data)}")
                if server.length or partial:
                    self.send_header("Content-Length", str(len(body)))
                else:
                    self.send_header("Connection", "close")
                self.send_header("Content-Disposition", 'attachment; filename="test file.bin"')
                self.end_headers()
                drop = server.drop_after and not server._dropped
                sent = 0
                try:
                    for i in range(0, len(body), 32 * 1024):
                        chunk = body[i:i + 32 * 1024]
                        if drop and sent + len(chunk) > server.drop_after:
                            server._dropped = True
                            self.wfile.write(chunk[: server.drop_after - sent])
                            self.close_connection = True
                            return
                        self.wfile.write(chunk)
                        sent += len(chunk)
                        if server.delay:
                            time.sleep(server.delay)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                if not server.length and not partial:
                    self.close_connection = True

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.httpd.daemon_threads = True
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}/files/test%20file.bin"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()
