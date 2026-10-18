"""SQLite persistence for the download list and queues."""

from __future__ import annotations

import json
import sqlite3
import threading
from pathlib import Path

from .models import Download
from .settings import data_dir

SCHEMA = """
CREATE TABLE IF NOT EXISTS downloads (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS queues (name TEXT PRIMARY KEY, data TEXT NOT NULL);
"""


class Store:
    def __init__(self, path: Path | str | None = None):
        path = Path(path) if path else data_dir() / "downloads.db"
        if str(path) != ":memory:":
            path.parent.mkdir(parents=True, exist_ok=True)
        self._db = sqlite3.connect(str(path), check_same_thread=False)
        self._lock = threading.Lock()
        with self._lock:
            self._db.executescript(SCHEMA)

    def load_downloads(self) -> list[Download]:
        with self._lock:
            rows = self._db.execute("SELECT data FROM downloads ORDER BY id").fetchall()
        return [Download.from_dict(json.loads(r[0])) for r in rows]

    def save_download(self, d: Download) -> None:
        self.save_downloads([d])

    def save_downloads(self, items: list[Download]) -> None:
        with self._lock, self._db:
            self._db.executemany(
                "INSERT OR REPLACE INTO downloads (id, data) VALUES (?, ?)",
                [(d.id, json.dumps(d.to_dict())) for d in items],
            )

    def delete_downloads(self, ids: list[int]) -> None:
        with self._lock, self._db:
            self._db.executemany("DELETE FROM downloads WHERE id = ?", [(i,) for i in ids])

    def next_id(self) -> int:
        with self._lock:
            row = self._db.execute("SELECT COALESCE(MAX(id), 0) + 1 FROM downloads").fetchone()
        return int(row[0])

    def load_queues(self) -> dict[str, dict]:
        with self._lock:
            rows = self._db.execute("SELECT name, data FROM queues").fetchall()
        return {name: json.loads(data) for name, data in rows}

    def save_queue(self, name: str, data: dict) -> None:
        with self._lock, self._db:
            self._db.execute("INSERT OR REPLACE INTO queues (name, data) VALUES (?, ?)", (name, json.dumps(data)))

    def delete_queue(self, name: str) -> None:
        with self._lock, self._db:
            self._db.execute("DELETE FROM queues WHERE name = ?", (name,))

    def close(self) -> None:
        with self._lock:
            self._db.close()
