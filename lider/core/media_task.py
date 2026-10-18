"""Runs a YouTube download: fresh stream URLs, segmented download of each stream, ffmpeg join.

Exposes the same surface as DownloadTask (start/stop/join/running/meter/
snapshot/set_speed_limit/error/on_finish), so queues, the list and the
progress dialog treat it like any other download.
"""

from __future__ import annotations

import glob
import os
import shutil
import subprocess
import threading
import time
from typing import Callable

from . import media
from .engine import DownloadTask, EngineOptions, HttpClient, SpeedLimiter, SpeedMeter, unique_path
from .models import Download, Segment, Status


def part_prefix(d: Download) -> str:
    return f"{d.id}-media"


def remove_parts(temp_dir: str, d: Download) -> None:
    for path in glob.glob(os.path.join(temp_dir, part_prefix(d) + "*")):
        try:
            os.remove(path)
        except OSError:
            pass


def reset_streams(d: Download) -> None:
    for s in (d.media or {}).get("streams", []):
        for key in ("segments", "size", "resumable", "done", "file", "downloaded"):
            s.pop(key, None)


class MediaTask:
    def __init__(
        self,
        download: Download,
        make_client: Callable[[str], HttpClient],
        options: EngineOptions,
        global_limiter: SpeedLimiter | None = None,
        on_finish: Callable[["MediaTask"], None] | None = None,
        resolver: Callable[[str, list[str]], list[dict]] = media.resolve_streams,
    ):
        self.d = download
        self.make_client = make_client
        self.options = options
        self.global_limiter = global_limiter
        self.on_finish = on_finish
        self.resolver = resolver
        self.error = ""
        self.status_text = ""
        self.inner: DownloadTask | None = None
        self._current = -1
        self._idle_meter = SpeedMeter()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    # -- DownloadTask-compatible surface
    @property
    def streams(self) -> list[dict]:
        return self.d.media["streams"]

    @property
    def meter(self) -> SpeedMeter:
        return self.inner.meter if self.inner else self._idle_meter

    @property
    def connections(self):
        return self.inner.connections if self.inner else []

    @property
    def running(self) -> bool:
        return bool(self._thread and self._thread.is_alive())

    def start(self) -> None:
        self._thread = threading.Thread(target=self._run, name=f"media-{self.d.id}", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self.inner:
            self.inner.stop()

    def join(self, timeout: float | None = None) -> None:
        if self._thread:
            self._thread.join(timeout)

    def set_speed_limit(self, kbps: int) -> None:
        self.d.speed_limit_kbps = kbps
        if self.inner:
            self.inner.set_speed_limit(kbps)

    def snapshot(self) -> dict:
        segments: list[Segment] = []
        conns = []
        offset = 0
        inner_snap = self.inner.snapshot() if self.inner else None
        for i, s in enumerate(self.streams):
            size = max(0, s.get("size", 0) or 0)
            if inner_snap is not None and i == self._current:
                parts = inner_snap["segments"]
                conns = [(n, rec, f"{s['kind'].title()}: {info}", alive, rate)
                         for n, rec, info, alive, rate in inner_snap["connections"]]
            elif s.get("done"):
                parts = [Segment(0, size, size)]
            else:
                parts = [Segment(*x) for x in s.get("segments", [])]
            segments += [Segment(p.start + offset, p.end + offset if p.end >= 0 else -1, p.pos + offset) for p in parts]
            offset += size
        rate = self.meter.rate()
        left = int((self.d.size - self.d.downloaded) / rate) if rate > 0 and self.d.size > 0 else -1
        return {"segments": segments, "connections": conns, "rate": rate, "time_left": left}

    # -- work
    def _done_bytes(self) -> int:
        return sum(max(0, s.get("size", 0) or 0) for s in self.streams if s.get("done"))

    def _run(self) -> None:
        d = self.d
        d.last_try = time.time()
        d.error = ""
        d.status = Status.CONNECTING
        self.status_text = "Getting the video address..."
        try:
            fresh = self.resolver(d.media["page_url"], [s["format_id"] for s in self.streams])
            for s, f in zip(self.streams, fresh):
                if s.get("size", -1) > 0 and f["size"] > 0 and f["size"] != s["size"]:
                    for key in ("segments", "done", "file", "downloaded"):
                        s.pop(key, None)  # the stream changed on YouTube's side: start it again
                s["url"], s["headers"], s["ext"] = f["url"], f["headers"], f["ext"]
                if f["size"] > 0 and not s.get("size"):
                    s["size"] = f["size"]
            self._update_totals()
            for i, s in enumerate(self.streams):
                if self._stop.is_set():
                    break
                if s.get("done"):
                    if os.path.exists(s.get("file", "")):
                        continue
                    for key in ("segments", "done", "file", "downloaded"):
                        s.pop(key, None)  # the finished part was deleted: fetch it again
                self._current = i
                ok = self._download_stream(s)
                refreshes = 0
                # YouTube sometimes refuses a stream URL (403); a freshly resolved one usually works.
                while not ok and not self._stop.is_set() and "HTTP 403" in d.error and refreshes < 2:
                    refreshes += 1
                    self.status_text = "Getting a new video address..."
                    self._stop.wait(1)
                    f = self.resolver(d.media["page_url"], [s["format_id"]])[0]
                    s["url"], s["headers"] = f["url"], f["headers"]
                    d.error = self.error = ""
                    ok = self._download_stream(s)
                if not ok:
                    break
            if self._stop.is_set():
                d.status = Status.PAUSED
            elif all(s.get("done") for s in self.streams):
                self._assemble()
            elif d.status != Status.ERROR:
                d.status = Status.ERROR
        except Exception as e:  # noqa: BLE001 - yt-dlp and ffmpeg raise many kinds; show the message
            self.error = d.error = str(e).replace("ERROR: ", "") or e.__class__.__name__
            d.status = Status.ERROR
        finally:
            self.inner = None
            self._current = -1
            self.status_text = ""
        if self.on_finish:
            self.on_finish(self)

    def _update_totals(self) -> None:
        sizes = [s.get("size", -1) or -1 for s in self.streams]
        if all(n > 0 for n in sizes):
            self.d.size = sum(sizes)

    def _download_stream(self, s: dict) -> bool:
        d = self.d
        name = f"{part_prefix(d)}-{s['kind']}"
        sub = Download(
            id=d.id, url=s["url"], filename=f"{name}.{s['ext']}", save_dir=self.options.temp_dir,
            size=s.get("size", -1) or -1, resumable=s.get("resumable"),
            segments=[Segment(*x) for x in s.get("segments", [])],
            speed_limit_kbps=d.speed_limit_kbps, headers=s["headers"],
        )
        if not sub.segments:
            stale = os.path.join(self.options.temp_dir, sub.filename)
            if os.path.exists(stale):
                os.remove(stale)
        client = self.make_client(s["headers"].get("User-Agent", ""))
        task = DownloadTask(sub, client, self.options, self.global_limiter, part_name=name)
        self.inner = task
        d.status = Status.DOWNLOADING
        self.status_text = f"Receiving {s['kind']}..."
        done_before = self._done_bytes()
        task.start()
        while task.running:
            d.downloaded = done_before + sub.downloaded
            if sub.size > 0 and s.get("size") != sub.size:
                s["size"] = sub.size
                self._update_totals()
            time.sleep(0.2)
        task.join()
        s["segments"] = [[x.start, x.end, x.pos] for x in sub.segments]
        s["resumable"] = sub.resumable
        s["size"] = sub.size
        self._update_totals()
        if sub.status == Status.COMPLETE:
            s["done"], s["file"] = True, sub.path
            d.downloaded = self._done_bytes()
            return True
        d.downloaded = done_before + sub.downloaded
        if sub.status == Status.PAUSED:
            return False
        self.error = d.error = f"{s['kind'].title()} stream: {sub.error or task.error or 'download failed'}"
        d.status = Status.ERROR
        return False

    def _assemble(self) -> None:
        d = self.d
        variant = d.media["variant"]
        d.status = Status.ASSEMBLING
        inputs = [s["file"] for s in self.streams]
        if len(inputs) == 2:
            self.status_text = "Merging video and audio..."
        elif variant.get("convert") == "mp3":
            self.status_text = "Converting to MP3..."
        else:
            self.status_text = "Finishing the file..."
        out = os.path.join(self.options.temp_dir, f"{part_prefix(d)}-out.{variant['ext']}")
        cmd = media.postprocess_command(inputs, out, variant)
        if cmd is None:
            if len(inputs) > 1 or variant.get("convert"):
                raise RuntimeError("ffmpeg is required to finish this download")
            shutil.move(inputs[0], out)
        else:
            proc = subprocess.run(cmd, capture_output=True, text=True)
            if proc.returncode != 0:
                raise RuntimeError(f"ffmpeg failed: {proc.stderr.strip()[-300:]}")
        os.makedirs(d.save_dir, exist_ok=True)
        target = unique_path(d.path)
        shutil.move(out, target)
        d.filename = os.path.basename(target)
        d.size = d.downloaded = os.path.getsize(target)
        remove_parts(self.options.temp_dir, d)
        d.status = Status.COMPLETE
