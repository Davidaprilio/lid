"""Multi-connection HTTP(S) downloader with dynamic segmentation.

Each connection asks for an open-ended range (``bytes=pos-``) and stops reading
when it reaches the end of its segment. When a connection finishes, it takes an
abandoned segment or splits the largest remaining one in half and continues
there, so all connections stay busy until the file is complete. Bytes are
written in place (``os.pwrite``) into a pre-sized part file in the temporary
directory, which is moved to the destination when the download completes.
"""

from __future__ import annotations

import base64
import os
import posixpath
import re
import shutil
import threading
import time
import urllib.error
import urllib.request
from collections import deque
from dataclasses import dataclass, field
from http.client import HTTPException
from typing import Callable
from urllib.parse import unquote, urlsplit

from .models import Download, Segment, Status

CHUNK = 1024 * 1024  # largest read when no speed limit applies; big reads keep the GIL out of the way
LIMITED_CHUNK = 64 * 1024  # smaller reads keep a speed limit smooth
MIN_SPLIT = 256 * 1024  # never create a segment smaller than this
FATAL_HTTP = {400, 401, 403, 404, 405, 410, 451}


class ResumeNotSupported(Exception):
    pass


# --------------------------------------------------------------------------- helpers

def parse_content_disposition(value: str | None) -> str | None:
    if not value:
        return None
    m = re.search(r"filename\*\s*=\s*([\w-]*)'[^']*'([^;]+)", value, re.I)
    if m:
        return unquote(m.group(2).strip().strip('"'), encoding=m.group(1) or "utf-8", errors="replace")
    m = re.search(r'filename\s*=\s*"([^"]*)"', value, re.I) or re.search(r"filename\s*=\s*([^;]+)", value, re.I)
    return m.group(1).strip() if m else None


def filename_from_url(url: str) -> str:
    name = posixpath.basename(unquote(urlsplit(url).path))
    return name or "index.html"


def safe_filename(name: str) -> str:
    name = re.sub(r"[/\\\x00-\x1f]", "_", name).strip().strip(".") or "download"
    stem, ext = os.path.splitext(name)
    return stem[: 240 - len(ext)] + ext


def unique_path(path: str) -> str:
    """``file.zip`` -> ``file (2).zip`` when the name is taken."""
    if not os.path.exists(path):
        return path
    stem, ext = os.path.splitext(path)
    n = 2
    while os.path.exists(f"{stem} ({n}){ext}"):
        n += 1
    return f"{stem} ({n}){ext}"


class SpeedLimiter:
    """Token bucket shared by every connection that should obey one limit."""

    def __init__(self, kbps: int = 0):
        self._lock = threading.Lock()
        self.set_kbps(kbps)

    def set_kbps(self, kbps: int) -> None:
        with self._lock:
            self.rate = max(0, int(kbps)) * 1024
            self._tokens = 0.0
            self._t = time.monotonic()

    def consume(self, n: int, stop: threading.Event) -> None:
        with self._lock:
            if self.rate <= 0:
                return
            now = time.monotonic()
            self._tokens = min(self.rate * 0.25, self._tokens + (now - self._t) * self.rate)
            self._t = now
            self._tokens -= n
            delay = -self._tokens / self.rate
        if delay > 0:
            stop.wait(delay)


class SpeedMeter:
    def __init__(self, window: float = 4.0):
        self.window = window
        self._samples: deque[tuple[float, int]] = deque()
        self._lock = threading.Lock()

    def add(self, n: int) -> None:
        now = time.monotonic()
        with self._lock:
            self._samples.append((now, n))
            while self._samples and now - self._samples[0][0] > self.window:
                self._samples.popleft()

    def rate(self) -> float:
        now = time.monotonic()
        with self._lock:
            while self._samples and now - self._samples[0][0] > self.window:
                self._samples.popleft()
            if not self._samples:
                return 0.0
            span = max(1.0, now - self._samples[0][0])
            return sum(n for _, n in self._samples) / span


# --------------------------------------------------------------------------- HTTP

@dataclass
class ProbeResult:
    url: str
    size: int
    resumable: bool
    filename: str
    content_type: str
    response: object = None


class HttpClient:
    def __init__(self, user_agent: str, proxy: str = "", timeout: float = 30.0):
        self.user_agent = user_agent
        self.timeout = timeout
        handlers = []
        if proxy:
            handlers.append(urllib.request.ProxyHandler({"http": proxy, "https": proxy}))
        self._opener = urllib.request.build_opener(*handlers)

    def open(self, url: str, start: int = 0, ranged: bool = True, headers: dict | None = None):
        if urlsplit(url).scheme == "ftp":
            return self._opener.open(url, timeout=self.timeout)
        h = {"User-Agent": self.user_agent, "Accept": "*/*", "Accept-Encoding": "identity"}
        if ranged:
            h["Range"] = f"bytes={start}-"
        h.update(headers or {})
        return self._opener.open(urllib.request.Request(url, headers=h), timeout=self.timeout)

    def probe(self, url: str, headers: dict | None = None, keep_open: bool = False) -> ProbeResult:
        resp = self.open(url, 0, True, headers)
        status = getattr(resp, "status", 200)
        hdrs = resp.headers
        size = -1
        resumable = False
        m = re.match(r"bytes\s+\d+-\d+/(\d+)", hdrs.get("Content-Range", "") or "")
        if status == 206 and m:
            size, resumable = int(m.group(1)), True
        elif hdrs.get("Content-Length"):
            size = int(hdrs["Content-Length"])
        final = resp.geturl() if hasattr(resp, "geturl") else url
        name = parse_content_disposition(hdrs.get("Content-Disposition")) or filename_from_url(final)
        result = ProbeResult(final, size, resumable, safe_filename(name), hdrs.get("Content-Type", ""))
        if keep_open:
            result.response = resp
        else:
            resp.close()
        return result


def request_headers(d: Download) -> dict:
    h = dict(d.headers)
    if d.referer:
        h["Referer"] = d.referer
    if d.username:
        token = base64.b64encode(f"{d.username}:{d.password}".encode()).decode()
        h["Authorization"] = f"Basic {token}"
    return h


# --------------------------------------------------------------------------- task

@dataclass
class EngineOptions:
    max_connections: int = 8
    retries: int = 10
    retry_delay: float = 3.0
    temp_dir: str = "/tmp"


@dataclass
class Connection:
    index: int
    segment: Segment | None = None
    status: str = "Connecting..."
    received: int = 0
    alive: bool = True
    thread: threading.Thread | None = field(default=None, repr=False)
    meter: SpeedMeter = field(default_factory=SpeedMeter, repr=False)


class DownloadTask:
    """Runs one download on background threads; the UI reads ``snapshot()``."""

    def __init__(
        self,
        download: Download,
        client: HttpClient,
        options: EngineOptions,
        global_limiter: SpeedLimiter | None = None,
        on_finish: Callable[["DownloadTask"], None] | None = None,
        part_name: str | None = None,
    ):
        self.d = download
        self.part_name = part_name or str(download.id)
        self.client = client
        self.options = options
        self.global_limiter = global_limiter
        self.file_limiter = SpeedLimiter(download.speed_limit_kbps)
        self.on_finish = on_finish
        self.meter = SpeedMeter()
        self.connections: list[Connection] = []
        self.error = ""
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._fd = -1
        self._splittable = False
        self._thread: threading.Thread | None = None

    # -- public API
    @property
    def part_path(self) -> str:
        return os.path.join(self.options.temp_dir, f"{self.part_name}.part")

    def start(self) -> None:
        self._thread = threading.Thread(target=self._run, name=f"dl-{self.d.id}", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()

    def join(self, timeout: float | None = None) -> None:
        if self._thread:
            self._thread.join(timeout)

    @property
    def running(self) -> bool:
        return bool(self._thread and self._thread.is_alive())

    def set_speed_limit(self, kbps: int) -> None:
        self.d.speed_limit_kbps = kbps
        self.file_limiter.set_kbps(kbps)

    def snapshot(self) -> dict:
        with self._lock:
            segs = [Segment(s.start, s.end, s.pos) for s in self.d.segments]
            conns = [(c.index, c.received, c.status, c.alive, c.meter.rate() if c.alive else 0.0)
                     for c in self.connections]
        rate = self.meter.rate()
        left = -1
        if rate > 0 and self.d.size > 0:
            left = int((self.d.size - self.d.downloaded) / rate)
        return {"segments": segs, "connections": conns, "rate": rate, "time_left": left}

    # -- thread bodies
    def _set_status(self, status: Status) -> None:
        self.d.status = status

    def _run(self) -> None:
        d = self.d
        d.last_try = time.time()
        d.error = ""
        self._set_status(Status.CONNECTING)
        first_response = None
        try:
            probe = self.client.probe(d.url, request_headers(d), keep_open=True)
            can_resume = (
                d.segments
                and d.resumable
                and probe.resumable
                and probe.size == d.size
                and os.path.exists(self.part_path)
            )
            if can_resume:
                probe.response.close()
            else:
                d.size, d.resumable = probe.size, probe.resumable
                d.segments = [Segment(0, probe.size if probe.size >= 0 else -1, 0)]
                first_response = probe.response
                if os.path.exists(self.part_path):
                    os.remove(self.part_path)
            d.downloaded = sum(s.pos - s.start for s in d.segments)
            self._splittable = bool(d.resumable and d.size > 0)

            os.makedirs(self.options.temp_dir, exist_ok=True)
            self._fd = os.open(self.part_path, os.O_RDWR | os.O_CREAT, 0o644)
            if d.size >= 0:
                os.ftruncate(self._fd, d.size)

            if self._stop.is_set():
                if first_response:
                    first_response.close()
            else:
                self._set_status(Status.DOWNLOADING)
                self._spawn_workers(first_response)
                for c in list(self.connections):
                    c.thread.join()
                # A late split may have added connections while we were joining.
                while any(c.thread.is_alive() for c in self.connections):
                    for c in list(self.connections):
                        c.thread.join()
        except urllib.error.HTTPError as e:
            self.error = f"HTTP {e.code} {e.reason}"
        except (OSError, HTTPException, ValueError) as e:
            self.error = str(getattr(e, "reason", e)) or e.__class__.__name__
        finally:
            if self._fd >= 0:
                os.close(self._fd)
                self._fd = -1
        self._finish()

    def _spawn_workers(self, first_response) -> None:
        d = self.d
        limit = self.options.max_connections if self._splittable else 1
        pending = [s for s in d.segments if not s.done]
        if not pending:
            if first_response:
                first_response.close()
            return
        with self._lock:
            for i, seg in enumerate(pending[:limit]):
                self._add_connection(seg, first_response if i == 0 else None)
            while self._splittable and len(self.connections) < limit:
                seg = self._split_largest()
                if seg is None:
                    break
                self._add_connection(seg, None)

    def _add_connection(self, seg: Segment, response) -> None:
        conn = Connection(index=len(self.connections) + 1, segment=seg)
        conn.thread = threading.Thread(target=self._worker, args=(conn, response), daemon=True)
        self.connections.append(conn)
        conn.thread.start()

    def _owned(self) -> set[int]:
        return {id(c.segment) for c in self.connections if c.segment is not None and c.alive}

    def _split_largest(self) -> Segment | None:
        """Caller holds the lock. Halve the segment with the most bytes left."""
        owned = self._owned()
        candidates = [s for s in self.d.segments if not s.done and id(s) in owned]
        if not candidates:
            return None
        best = max(candidates, key=lambda s: s.remaining)
        if best.remaining < 2 * MIN_SPLIT:
            return None
        mid = best.pos + best.remaining // 2
        new = Segment(mid, best.end, mid)
        best.end = mid
        self.d.segments.append(new)
        self.d.segments.sort(key=lambda s: s.start)
        return new

    def _next_work(self) -> Segment | None:
        """Caller holds the lock. Take an abandoned segment first, else split."""
        owned = self._owned()
        for s in self.d.segments:
            if not s.done and id(s) not in owned:
                return s
        return self._split_largest() if self._splittable else None

    def _worker(self, conn: Connection, response) -> None:
        try:
            while not self._stop.is_set() and conn.segment is not None:
                result = self._fetch(conn, conn.segment, response)
                response = None
                with self._lock:
                    if result != "done" or self._stop.is_set():
                        conn.segment = None
                        break
                    conn.segment = self._next_work()
            if not self._stop.is_set() and conn.status.startswith(("Receiving", "Connecting")):
                conn.status = "Finished"
        finally:
            if response is not None:
                response.close()
            conn.alive = False

    def _fetch(self, conn: Connection, seg: Segment, response) -> str:
        attempts = 0
        headers = request_headers(self.d)
        while not self._stop.is_set():
            err = ""
            try:
                if response is None:
                    conn.status = "Connecting..."
                    ranged = self._splittable or seg.pos > 0
                    response = self.client.open(self.d.url, seg.pos, ranged, headers)
                    if ranged and getattr(response, "status", 200) != 206:
                        raise ResumeNotSupported("server ignored the Range header")
                conn.status = "Receiving data..."
                # read1 returns what has arrived (up to the chunk size), so slow links still report progress often
                read = getattr(response, "read1", response.read)
                while not self._stop.is_set():
                    limited = self.file_limiter.rate > 0 or (self.global_limiter is not None and self.global_limiter.rate > 0)
                    step = LIMITED_CHUNK if limited else CHUNK
                    with self._lock:
                        want = seg.end - seg.pos if seg.end >= 0 else step
                    if want <= 0:
                        return "done"
                    data = read(min(step, want))
                    if not data:
                        if seg.end < 0:
                            with self._lock:
                                seg.end = seg.pos
                                self.d.size = seg.pos
                            return "done"
                        raise ConnectionError("connection closed by the server")
                    if self.global_limiter:
                        self.global_limiter.consume(len(data), self._stop)
                    self.file_limiter.consume(len(data), self._stop)
                    with self._lock:
                        allowed = len(data) if seg.end < 0 else max(0, min(len(data), seg.end - seg.pos))
                        offset = seg.pos
                        seg.pos += allowed
                        self.d.downloaded += allowed
                        conn.received += allowed
                    if allowed:
                        os.pwrite(self._fd, data[:allowed], offset)
                        self.meter.add(allowed)
                        conn.meter.add(allowed)
                return "stopped"
            except ResumeNotSupported as e:
                conn.status = f"Error: {e}"
                self.error = str(e)
                return "failed"
            except urllib.error.HTTPError as e:
                if e.code in FATAL_HTTP:
                    conn.status = self.error = f"HTTP {e.code} {e.reason}"
                    return "failed"
                err = f"HTTP {e.code}"
            except (OSError, HTTPException) as e:
                err = str(getattr(e, "reason", e)) or e.__class__.__name__
            finally:
                if response is not None:
                    response.close()
                    response = None
            attempts += 1
            if attempts > self.options.retries:
                conn.status = f"Error: {err}"
                self.error = err
                return "failed"
            if not self.d.resumable:
                # The server cannot continue a partial transfer: start over.
                with self._lock:
                    self.d.downloaded -= seg.pos - seg.start
                    seg.pos = seg.start
                    conn.received = 0
            conn.status = f"Reconnecting ({attempts}/{self.options.retries})..."
            self._stop.wait(self.options.retry_delay)
        return "stopped"

    def _finish(self) -> None:
        d = self.d
        # One connection may have failed while the others took over its bytes,
        # so completeness is decided by the segments, not by self.error.
        complete = bool(d.segments) and all(s.done for s in d.segments)
        if complete and not self._stop.is_set():
            self._set_status(Status.ASSEMBLING)
            try:
                os.makedirs(d.save_dir, exist_ok=True)
                target = unique_path(d.path)
                shutil.move(self.part_path, target)
                d.filename = os.path.basename(target)
                d.size = d.downloaded
                d.segments = []
                self._set_status(Status.COMPLETE)
            except OSError as e:
                self.error = d.error = f"Cannot save the file: {e}"
                self._set_status(Status.ERROR)
        elif self._stop.is_set():
            self._set_status(Status.PAUSED)
        else:
            d.error = self.error or "Download failed"
            self._set_status(Status.ERROR)
        if self.on_finish:
            self.on_finish(self)
