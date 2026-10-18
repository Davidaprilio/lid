"""Download records shared by the engine, the store and the UI."""

from __future__ import annotations

import os
import time
from dataclasses import asdict, dataclass, field
from enum import Enum

MAIN_QUEUE = "Main download queue"
SYNC_QUEUE = "Synchronization queue"


class Status(str, Enum):
    QUEUED = "queued"  # added but never started (the list shows an empty status)
    CONNECTING = "connecting"
    DOWNLOADING = "downloading"
    PAUSED = "paused"  # stopped by the user; the list shows the percentage
    ASSEMBLING = "assembling"
    COMPLETE = "complete"
    ERROR = "error"

    @property
    def active(self) -> bool:
        return self in (Status.CONNECTING, Status.DOWNLOADING, Status.ASSEMBLING)


@dataclass
class Segment:
    """Byte range [start, end) of the file; ``pos`` is the next byte to write.

    ``end`` is -1 while the size is unknown (single streaming connection).
    """

    start: int
    end: int
    pos: int

    @property
    def remaining(self) -> int:
        return -1 if self.end < 0 else max(0, self.end - self.pos)

    @property
    def done(self) -> bool:
        return self.end >= 0 and self.pos >= self.end


@dataclass
class Download:
    id: int
    url: str
    filename: str
    save_dir: str
    category: str = "General"
    queue: str = ""
    description: str = ""
    referer: str = ""
    parent_page: str = ""
    size: int = -1
    downloaded: int = 0
    status: Status = Status.QUEUED
    resumable: bool | None = None
    connections: int = 0  # 0 = use the default from Options->Connection
    segments: list[Segment] = field(default_factory=list)
    date_added: float = field(default_factory=time.time)
    last_try: float = 0.0
    error: str = ""
    username: str = ""
    password: str = ""
    speed_limit_kbps: int = 0  # per-file limiter from the progress dialog, 0 = off
    queue_order: int = 0
    headers: dict[str, str] = field(default_factory=dict)  # extra request headers
    media: dict | None = None  # set for YouTube downloads: chosen variant and per-stream state

    @property
    def path(self) -> str:
        return os.path.join(self.save_dir, self.filename)

    @property
    def progress(self) -> float:
        if self.size > 0:
            return min(1.0, self.downloaded / self.size)
        return 1.0 if self.status == Status.COMPLETE else 0.0

    def to_dict(self) -> dict:
        d = asdict(self)
        d["status"] = self.status.value
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "Download":
        d = dict(d)
        d["status"] = Status(d.get("status", Status.QUEUED))
        d["segments"] = [Segment(**s) for s in d.get("segments", [])]
        known = cls.__dataclass_fields__
        return cls(**{k: v for k, v in d.items() if k in known})
