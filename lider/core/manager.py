"""Owns the download list, running tasks, queues and the scheduler.

The UI calls ``poll()`` on a timer; finished tasks, queue progress and
scheduled starts/stops are handled there on the UI thread, so the engine
threads never touch Qt.
"""

from __future__ import annotations

import os
import queue
import time
from dataclasses import asdict, dataclass, field
from datetime import datetime, timedelta
from urllib.parse import urlsplit

from . import categories
from .engine import DownloadTask, EngineOptions, HttpClient, ProbeResult, SpeedLimiter, request_headers
from .media_task import MediaTask, remove_parts, reset_streams
from .models import MAIN_QUEUE, SYNC_QUEUE, Download, Status
from .settings import Settings
from .store import Store


@dataclass
class QueueSchedule:
    """Fields of the Scheduler's "Schedule" and "Files in the queue" tabs."""

    max_parallel: int = 1
    periodic: bool = False  # "Periodic synchronization" instead of "One-time downloading"
    start_on_startup: bool = False
    start_enabled: bool = False
    start_time: str = "23:00"
    daily: bool = False
    once_date: str = ""  # YYYY-MM-DD for "Once at"
    days: list[int] = field(default_factory=lambda: list(range(7)))  # 0 = Monday
    every_hours: int = 2
    every_minutes: int = 0
    stop_enabled: bool = False
    stop_time: str = "07:30"
    retries_enabled: bool = True
    retries: int = 10
    open_when_done: str = ""
    exit_when_done: bool = False
    shutdown_when_done: bool = False

    def next_start(self, now: datetime) -> datetime | None:
        """When "Start download at" fires next, to the minute, the way the scheduler checks it; None when it won't."""
        if not self.start_enabled:
            return None
        h, m = (int(x) for x in self.start_time.split(":"))
        now = now.replace(second=0, microsecond=0)
        today = now.replace(hour=h, minute=m)
        if self.daily:
            return next((t for t in (today + timedelta(days=i) for i in range(8))
                         if t >= now and t.weekday() in self.days), None)
        if self.once_date:
            t = datetime.strptime(self.once_date, "%Y-%m-%d").replace(hour=h, minute=m)
            return t if t >= now else None
        return today if today >= now else today + timedelta(days=1)


@dataclass
class Queue:
    name: str
    schedule: QueueSchedule = field(default_factory=QueueSchedule)
    running: bool = False
    next_run: float = 0.0
    failures: dict[int, int] = field(default_factory=dict)
    _fired: set[str] = field(default_factory=set)


@dataclass
class Event:
    kind: str  # complete | error | queue_started | queue_done | changed
    download_id: int | None = None
    queue: str | None = None
    message: str = ""


class DownloadManager:
    def __init__(self, settings: Settings, store: Store):
        self.settings = settings
        self.store = store
        self.downloads: dict[int, Download] = {}
        for d in store.load_downloads():
            if d.status.active:
                d.status = Status.PAUSED  # the app was closed while downloading
            self.downloads[d.id] = d
        self.tasks: dict[int, DownloadTask] = {}
        self.queues: dict[str, Queue] = {}
        for name, data in store.load_queues().items():
            self.queues[name] = Queue(name, QueueSchedule(**data.get("schedule", {})))
        for name, periodic in ((MAIN_QUEUE, False), (SYNC_QUEUE, True)):
            if name not in self.queues:
                self.queues[name] = Queue(name, QueueSchedule(periodic=periodic))
        self.limiter = SpeedLimiter(settings.speed_limit_kbps if settings.speed_limiter_on else 0)
        self._finished: queue.Queue[DownloadTask] = queue.Queue()
        self._next_id = store.next_id()
        self._started = False
        self._last_save = time.monotonic()

    # ----------------------------------------------------------------- adding
    def client(self) -> HttpClient:
        return HttpClient(self.settings.user_agent, self.settings.proxy, self.settings.timeout)

    def probe(self, url: str, referer: str = "", username: str = "", password: str = "",
              headers: dict | None = None) -> ProbeResult:
        tmp = Download(0, url, "", "", referer=referer, username=username, password=password, headers=headers or {})
        return self.client().probe(url, request_headers(tmp))

    def find_duplicate(self, url: str) -> Download | None:
        for d in self.downloads.values():
            if d.url == url:
                return d
        return None

    def add(
        self,
        url: str,
        filename: str,
        save_dir: str | None = None,
        category: str | None = None,
        description: str = "",
        referer: str = "",
        parent_page: str = "",
        username: str = "",
        password: str = "",
        size: int = -1,
        resumable: bool | None = None,
        queue_name: str = "",
        start: bool = False,
        media: dict | None = None,
        headers: dict | None = None,
    ) -> Download:
        cat = self.settings.category(category) if category else categories.classify(self.settings, filename, url)
        d = Download(
            id=self._next_id,
            url=url,
            filename=filename,
            save_dir=save_dir or cat.save_dir,
            category=cat.name,
            description=description,
            referer=referer,
            parent_page=parent_page,
            username=username,
            password=password,
            size=size,
            resumable=resumable,
            media=media,
            headers=headers or {},
        )
        self._next_id += 1
        self.downloads[d.id] = d
        if queue_name:
            self.set_queue([d.id], queue_name)
        self.store.save_download(d)
        if start:
            self.start(d.id)
        return d

    # ----------------------------------------------------------------- control
    def task(self, download_id: int) -> DownloadTask | MediaTask | None:
        return self.tasks.get(download_id)

    def is_running(self, download_id: int) -> bool:
        t = self.tasks.get(download_id)
        return bool(t and t.running)

    def _options(self, d: Download) -> EngineOptions:
        host = urlsplit(d.url).hostname or ""
        return EngineOptions(
            max_connections=d.connections or self.settings.connections_for(host),
            retries=self.settings.retries,
            retry_delay=self.settings.retry_delay,
            temp_dir=self.settings.temp_dir,
        )

    def start(self, download_id: int) -> DownloadTask | MediaTask | None:
        d = self.downloads.get(download_id)
        if d is None or self.is_running(download_id) or d.status == Status.COMPLETE:
            return self.tasks.get(download_id)
        if d.media:
            s = self.settings
            task = MediaTask(d, lambda ua: HttpClient(ua or s.user_agent, s.proxy, s.timeout), self._options(d),
                             self.limiter, on_finish=self._finished.put)
        else:
            task = DownloadTask(d, self.client(), self._options(d), self.limiter, on_finish=self._finished.put)
        self.tasks[d.id] = task
        task.start()
        return task

    def stop(self, download_id: int) -> None:
        t = self.tasks.get(download_id)
        if t:
            t.stop()

    def stop_all(self) -> None:
        for t in list(self.tasks.values()):
            t.stop()
        for q in self.queues.values():
            q.running = False

    def redownload(self, download_id: int, overwrite: bool = False, start: bool = True) -> None:
        d = self.downloads[download_id]
        if self.is_running(download_id):
            return
        if overwrite and os.path.exists(d.path):
            os.remove(d.path)
        if d.media:
            reset_streams(d)
            remove_parts(self.settings.temp_dir, d)
            size = d.media["variant"].get("size", -1)
        else:
            size = -1
        d.status, d.downloaded, d.segments, d.size, d.resumable, d.error = Status.QUEUED, 0, [], size, None, ""
        self.store.save_download(d)
        if start:
            self.start(download_id)

    def remove(self, ids: list[int], delete_files: bool = False) -> None:
        for i in ids:
            t = self.tasks.pop(i, None)
            if t:
                t.stop()
                t.join(5)
            d = self.downloads.pop(i, None)
            if d is None:
                continue
            part = os.path.join(self.settings.temp_dir, f"{d.id}.part")
            if os.path.exists(part):
                os.remove(part)
            if d.media:
                remove_parts(self.settings.temp_dir, d)
            if delete_files and d.status == Status.COMPLETE and os.path.exists(d.path):
                os.remove(d.path)
        self.store.delete_downloads(ids)

    def remove_completed(self) -> list[int]:
        ids = [d.id for d in self.downloads.values() if d.status == Status.COMPLETE]
        self.remove(ids)
        return ids

    def update(self, d: Download) -> None:
        self.store.save_download(d)

    def set_speed_limiter(self, on: bool, kbps: int | None = None) -> None:
        if kbps is not None:
            self.settings.speed_limit_kbps = kbps
        self.settings.speed_limiter_on = on
        self.limiter.set_kbps(self.settings.speed_limit_kbps if on else 0)

    # ----------------------------------------------------------------- queues
    def queue_items(self, name: str) -> list[Download]:
        return sorted((d for d in self.downloads.values() if d.queue == name), key=lambda d: d.queue_order)

    def set_queue(self, ids: list[int], name: str) -> None:
        order = max((d.queue_order for d in self.queue_items(name)), default=0) if name else 0
        for i in ids:
            d = self.downloads[i]
            if d.queue != name:
                order += 1
                d.queue, d.queue_order = name, order
                self.store.save_download(d)

    def move_in_queue(self, download_id: int, delta: int) -> None:
        d = self.downloads[download_id]
        items = self.queue_items(d.queue)
        i = items.index(d)
        j = max(0, min(len(items) - 1, i + delta))
        items.insert(j, items.pop(i))
        for n, item in enumerate(items, 1):
            item.queue_order = n
        self.store.save_downloads(items)

    def add_queue(self, name: str) -> Queue:
        q = self.queues.setdefault(name, Queue(name))
        self.save_queue(name)
        return q

    def delete_queue(self, name: str) -> None:
        if name in (MAIN_QUEUE, SYNC_QUEUE):
            return
        self.stop_queue(name)
        for d in self.queue_items(name):
            d.queue = ""
            self.store.save_download(d)
        self.queues.pop(name, None)
        self.store.delete_queue(name)

    def save_queue(self, name: str) -> None:
        self.store.save_queue(name, {"schedule": asdict(self.queues[name].schedule)})

    def start_queue(self, name: str) -> list[Event]:
        q = self.queues[name]
        if q.running:
            return []
        q.running = True
        q.failures.clear()
        if q.schedule.periodic:
            for d in self.queue_items(name):
                if d.status == Status.COMPLETE:
                    self.redownload(d.id, overwrite=True, start=False)
        return [Event("queue_started", queue=name)] + self._process_queue(q)

    def stop_queue(self, name: str) -> None:
        q = self.queues[name]
        q.running = False
        for d in self.queue_items(name):
            self.stop(d.id)

    def _process_queue(self, q: Queue) -> list[Event]:
        if not q.running:
            return []
        items = self.queue_items(q.name)
        active = [d for d in items if self.is_running(d.id)]
        limit = q.schedule.retries if q.schedule.retries_enabled else 0
        waiting = [
            d for d in items
            if not self.is_running(d.id)
            and d.status != Status.COMPLETE
            and (d.status != Status.ERROR or q.failures.get(d.id, 0) <= limit)
        ]
        for d in waiting[: max(0, q.schedule.max_parallel - len(active))]:
            self.start(d.id)
            active.append(d)
        if active:
            return []
        q.running = False
        if q.schedule.periodic:
            q.next_run = time.time() + q.schedule.every_hours * 3600 + q.schedule.every_minutes * 60
        return [Event("queue_done", queue=q.name)]

    # ----------------------------------------------------------------- scheduler
    @staticmethod
    def _due(time_str: str, now: datetime) -> bool:
        return now.strftime("%H:%M") == time_str

    def _tick_schedules(self, now: datetime) -> list[Event]:
        events: list[Event] = []
        stamp = now.strftime("%Y-%m-%d %H:%M")
        for q in self.queues.values():
            s = q.schedule
            if s.start_enabled and not q.running and self._due(s.start_time, now) and ("start " + stamp) not in q._fired:
                day_ok = (now.weekday() in s.days) if s.daily else (not s.once_date or s.once_date == now.strftime("%Y-%m-%d"))
                if day_ok:
                    q._fired.add("start " + stamp)
                    events += self.start_queue(q.name)
            if s.stop_enabled and q.running and self._due(s.stop_time, now) and ("stop " + stamp) not in q._fired:
                q._fired.add("stop " + stamp)
                self.stop_queue(q.name)
                events.append(Event("queue_done", queue=q.name, message="stopped by schedule"))
            if s.periodic and q.next_run and not q.running and time.time() >= q.next_run:
                q.next_run = 0
                events += self.start_queue(q.name)
        return events

    # ----------------------------------------------------------------- poll
    def poll(self, now: datetime | None = None) -> list[Event]:
        events: list[Event] = []
        if not self._started:
            self._started = True
            for q in self.queues.values():
                if q.schedule.start_on_startup:
                    events += self.start_queue(q.name)
        while True:
            try:
                task = self._finished.get_nowait()
            except queue.Empty:
                break
            d = task.d
            if self.tasks.get(d.id) is task:
                del self.tasks[d.id]
            if d.id not in self.downloads:
                continue
            self.store.save_download(d)
            if d.status == Status.COMPLETE:
                events.append(Event("complete", d.id))
            elif d.status == Status.ERROR:
                q = self.queues.get(d.queue)
                if q and q.running:
                    q.failures[d.id] = q.failures.get(d.id, 0) + 1
                events.append(Event("error", d.id, message=d.error))
            else:
                events.append(Event("changed", d.id))
        for q in self.queues.values():
            events += self._process_queue(q)
        events += self._tick_schedules(now or datetime.now())
        if time.monotonic() - self._last_save > 5 and self.tasks:
            self.store.save_downloads([t.d for t in self.tasks.values()])
            self._last_save = time.monotonic()
        return events

    def shutdown(self) -> None:
        for t in list(self.tasks.values()):
            t.stop()
        for t in list(self.tasks.values()):
            t.join(10)
        self.store.save_downloads(list(self.downloads.values()))
