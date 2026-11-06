"""Write a LID 1.2 profile with the Python code, for the Rust port's compatibility tests.

    python3 tests/golden/make_golden.py

It writes into tests/golden/v1.2/:
  config/lider/settings.json         settings with non-default values
  config/settings-legacy.json        settings from before the themes (dark_mode, toolbar_large)
  data/lider/downloads.db            every status, segments, media, checksum fields, queues
  cache/lider/parts/<id>.part        part files of the unfinished downloads (temp_dir points at
                                     /home/lid-user/.cache/lider/parts, so tests copy them there)
  expected.json                      what the 1.2 code reads back from those files

The Rust tests load the same files and must produce expected.json. Re-run only to change the samples;
the files are committed so the format of 1.2 stays frozen after the Python code is gone.
"""

from __future__ import annotations

import json
import os
import shutil
import sys
from dataclasses import asdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent / "v1.2"
HOME = "/home/lid-user"  # fixed, so the default category folders are the same on every machine

os.environ["HOME"] = HOME
os.environ["XDG_CONFIG_HOME"] = str(OUT / "config")
os.environ["XDG_DATA_HOME"] = str(OUT / "data")
os.environ["XDG_CACHE_HOME"] = f"{HOME}/.cache"  # default temp_dir; the part files go to OUT/cache
sys.path.insert(0, str(ROOT))

from lider.core import settings as settings_mod  # noqa: E402
from lider.core.manager import QueueSchedule  # noqa: E402
from lider.core.models import MAIN_QUEUE, SYNC_QUEUE, Download, Segment, Status  # noqa: E402
from lider.core.settings import Category, Settings  # noqa: E402
from lider.core.store import Store  # noqa: E402

T0 = 1793000000.0  # a fixed "date added"
DL = f"{HOME}/Downloads"
MiB = 1024  # small "MiB" keeps the committed part files tiny; sizes only need to be consistent


def make_settings() -> Settings:
    s = Settings()
    s.max_connections = 16
    s.server_limits = {"*.example.com": 4, "files.test": 2}
    s.retries = 5
    s.retry_delay = 1.5
    s.timeout = 20.0
    s.proxy = "http://127.0.0.1:8080"
    s.speed_limiter_on = True
    s.speed_limit_kbps = 512
    s.theme = "dark"
    s.follow_system = False
    s.density = "compact"
    s.toolbar_labels = False
    s.menubar = "custom"
    s.show_details = False
    s.list_font = "Inter,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1"
    s.window_geometry = "AdnQywADAAAAAAAAAAAAAAAAA+cAAAIvAAAAAAAAAAAAAAPnAAACLwAAAAACAAAAB4AAAAAAAAAAAAAAA+cAAAIv"
    s.sort_key = "size"
    s.sort_desc = True
    s.sidebar_width = 260
    s.search_flags = 0x05
    s.search_string = "ubuntu"
    s.duplicate_action = "number"
    s.checksum_mismatch = "keep"
    s.media_kind = "audio"
    s.media_quality = "MP3"
    s.categories[5].extensions.append("vob")
    s.categories.append(Category("Images", ["png", "jpg"], f"{DL}/Images", ["*.imgur.com"], True))
    # A reordered column list with one column hidden, as the Columns dialog writes it.
    s.columns = [[k, v, w] for k, _, v, w in settings_mod.DEFAULT_COLUMNS]
    s.columns[2], s.columns[3] = s.columns[3], s.columns[2]
    s.columns[7][1] = False
    return s


def downloads() -> list[Download]:
    out = []

    def add(**kw) -> Download:
        d = Download(id=len(out) + 1, date_added=T0 + len(out), **kw)
        out.append(d)
        return d

    add(url="https://example.com/never-started.iso", filename="never-started.iso", save_dir=f"{DL}/Programs",
        category="Programs", queue=MAIN_QUEUE, queue_order=1)
    add(url="https://example.com/big.tar.gz", filename="big.tar.gz", save_dir=f"{DL}/Compressed",
        category="Compressed", status=Status.PAUSED, size=10 * MiB, downloaded=3 * MiB, resumable=True,
        connections=4, last_try=T0 + 100, speed_limit_kbps=300,
        segments=[Segment(0, 5 * MiB, 2 * MiB), Segment(5 * MiB, 10 * MiB, 6 * MiB)])
    add(url="https://files.test/stream.bin", filename="stream.bin", save_dir=DL, status=Status.ERROR,
        size=-1, downloaded=12345, resumable=False, error="HTTP Error 503: Service Unavailable",
        segments=[Segment(0, -1, 12345)], last_try=T0 + 200)
    add(url="https://example.com/doc.pdf", filename="doc.pdf", save_dir=f"{DL}/Documents", category="Documents",
        status=Status.COMPLETE, size=2048, downloaded=2048, resumable=True, description="manual",
        referer="https://example.com/", parent_page="https://example.com/docs",
        checksum="sha256:" + "ab" * 32, checksum_result="verified", checksum_actual="ab" * 32,
        checksum_checked=T0 + 300, checksum_seconds=0.25)
    add(url="https://example.com/bad.zip", filename="bad.zip", save_dir=f"{DL}/Compressed", category="Compressed",
        status=Status.COMPLETE, size=4096, downloaded=4096, resumable=True,
        checksum="md5:" + "0" * 32, checksum_result="mismatch", checksum_actual="f" * 32,
        checksum_retried=True, checksum_force=True)
    add(url="https://login.test/private.mkv", filename="privé — 映画.mkv", save_dir=f"{DL}/Video", category="Video",
        status=Status.DOWNLOADING, size=3 * MiB, downloaded=MiB, resumable=True, username="user",
        password="secret", headers={"Cookie": "a=1; b=2", "User-Agent": "Browser/1.0"},
        queue=SYNC_QUEUE, queue_order=2, segments=[Segment(0, 3 * MiB, MiB)])
    for i, st in enumerate((Status.CONNECTING, Status.ASSEMBLING, Status.VERIFYING)):
        add(url=f"https://example.com/state-{st.value}.bin", filename=f"state-{st.value}.bin", save_dir=DL,
            status=st, size=1024, downloaded=1024 if st != Status.CONNECTING else 0, queue="Nightly",
            queue_order=i + 1)
    add(url="https://www.youtube.com/watch?v=dQw4w9WgXcQ", filename="Video title.mp4", save_dir=f"{DL}/Video",
        category="Video", status=Status.PAUSED, size=50 * MiB, downloaded=20 * MiB,
        media={
            "source": "youtube", "page_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "video_id": "dQw4w9WgXcQ", "title": "Video title", "uploader": "Uploader", "variant": {
                "kind": "video", "quality": "1080p", "label": "1080p MP4", "ext": "mp4", "video_format": "137",
                "audio_format": "140", "size": 50 * MiB, "height": 1080, "convert": ""},
            "streams": [
                {"kind": "video", "format_id": "137", "url": "https://rr1.example/v", "headers": {"User-Agent": "yt"},
                 "ext": "mp4", "size": 45 * MiB, "resumable": True, "segments": [[0, 45 * MiB, 20 * MiB]]},
                {"kind": "audio", "format_id": "140", "url": "https://rr1.example/a", "headers": {}, "ext": "m4a",
                 "size": 5 * MiB, "resumable": True, "segments": [[0, 5 * MiB, 5 * MiB]], "done": True,
                 "file": "/home/lid-user/.cache/lider/parts/10-media-1.m4a", "downloaded": 5 * MiB},
            ]})
    return out


def queues() -> dict[str, QueueSchedule]:
    return {
        MAIN_QUEUE: QueueSchedule(),
        SYNC_QUEUE: QueueSchedule(periodic=True, every_hours=6, every_minutes=30, start_on_startup=True),
        "Nightly": QueueSchedule(max_parallel=3, start_enabled=True, start_time="01:30", daily=True,
                                 days=[0, 2, 4], stop_enabled=True, stop_time="06:00", retries=3,
                                 open_when_done="/usr/bin/true", exit_when_done=True),
        "Once": QueueSchedule(start_enabled=True, once_date="2026-12-24", start_time="08:00",
                              retries_enabled=False, shutdown_when_done=True),
    }


def main() -> None:
    shutil.rmtree(OUT, ignore_errors=True)
    s = make_settings()
    settings_mod.save(s)
    legacy = OUT / "config" / "settings-legacy.json"
    legacy.write_text(json.dumps({"dark_mode": True, "toolbar_large": False, "max_connections": 2,
                                  "no_longer_used_key": 1}, indent=2), encoding="utf-8")

    store = Store()
    items = downloads()
    store.save_downloads(items)
    for name, sched in queues().items():
        store.save_queue(name, {"schedule": asdict(sched)})
    store.close()

    parts = OUT / "cache" / "lider" / "parts"
    parts.mkdir(parents=True)
    for d in items:
        if d.status not in (Status.COMPLETE, Status.QUEUED) and not d.media and d.size > 0:
            with open(parts / f"{d.id}.part", "wb") as f:
                f.truncate(d.size)  # sparse, pre-sized the way the engine leaves it
    with open(parts / "10-media-0.mp4", "wb") as f:
        f.truncate(45 * MiB)

    # What 1.2 reads back: the contract for the Rust loader.
    store = Store()
    expected = {
        "settings": asdict(settings_mod.load()),
        "settings_legacy": asdict(settings_mod.load(legacy)),
        "settings_missing": asdict(settings_mod.load(OUT / "config" / "absent.json")),
        "downloads": [d.to_dict() for d in store.load_downloads()],
        "queues": store.load_queues(),
        "next_id": store.next_id(),
    }
    store.close()
    (OUT / "expected.json").write_text(json.dumps(expected, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
