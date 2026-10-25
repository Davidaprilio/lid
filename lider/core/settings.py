"""Application options and their defaults."""

from __future__ import annotations

import json
import os
from dataclasses import asdict, dataclass, field, fields
from pathlib import Path

CONNECTION_CHOICES = (1, 2, 4, 8, 16, 24, 32)

DEFAULT_FILE_TYPES = (
    "3GP 7Z AAC ACE AIF APK ARJ ASF AVI BIN BZ2 EXE GZ GZIP IMG ISO LZH M4A M4V MKV MOV MP3 MP4 "
    "MPA MPE MPEG MPG MSI MSU OGG OGV PDF PLJ PPS PPT QT R0* R1* RA RAR RM RMVB SEA SIT SITX TAR "
    "TIF TIFF WAV WMA WMV Z ZIP"
)

DEFAULT_USER_AGENT = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36"

_DEFAULT_CATEGORIES = [
    ("Compressed", "zip rar r0* r1* arj gz sit sitx sea ace bz2 7z"),
    ("Documents", "doc pdf ppt pps docx pptx"),
    ("Music", "mp3 wav wma mpa ram ra aac aif m4a tsa"),
    ("Programs", "exe msi"),
    ("Video", "avi mpg mpe mpeg asf wmv mov qt rm mp4 flv m4v webm ogv ogg mkv ts tsv"),
]

# (key, title, visible by default, width) in the default column order.
DEFAULT_COLUMNS = [
    ("queue", "Q", True, 34),
    ("filename", "File Name", True, 280),
    ("size", "Size", True, 92),
    ("status", "Status", True, 180),
    ("time_left", "Time left", True, 100),
    ("rate", "Transfer rate", True, 120),
    ("last_try", "Last Try Date", True, 130),
    ("description", "Description", True, 200),
    ("date_added", "Date Added", False, 130),
    ("save_to", "Save To", False, 200),
    ("referer", "Referer", False, 200),
    ("parent_page", "Parent Web Page", False, 200),
]


def config_dir() -> Path:
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "lider"


def data_dir() -> Path:
    return Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local" / "share") / "lider"


def cache_dir() -> Path:
    return Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "lider"


LEGACY_NAMES = ("lidman", "lindm")  # folder names used by earlier builds, newest first


def migrate_legacy_dirs() -> None:
    """Move settings, the download list and partial files from an earlier build's folders."""
    for new in (config_dir(), data_dir(), cache_dir()):
        for name in LEGACY_NAMES:
            old = new.with_name(name)
            if not old.is_dir():
                continue
            if not new.exists():
                old.rename(new)
                continue
            # The new folder may already exist (e.g. the browser helper was set up first): merge.
            for item in old.iterdir():
                if not (new / item.name).exists():
                    item.rename(new / item.name)
            try:
                old.rmdir()
            except OSError:
                pass  # something with the same name already existed in the new folder; leave it
    cfg = config_dir() / "settings.json"
    try:
        text = cfg.read_text(encoding="utf-8")
    except OSError:
        return
    changed = text
    for name in LEGACY_NAMES:
        changed = changed.replace(str(cache_dir().with_name(name)), str(cache_dir()))
    if changed != text:
        cfg.write_text(changed, encoding="utf-8")


def downloads_root() -> str:
    return str(Path.home() / "Downloads")


@dataclass
class Category:
    name: str
    extensions: list[str] = field(default_factory=list)
    save_dir: str = ""
    sites: list[str] = field(default_factory=list)
    remember_last: bool = False


def default_categories() -> list[Category]:
    root = downloads_root()
    cats = [Category("General", [], root)]
    cats += [Category(n, exts.split(), os.path.join(root, n)) for n, exts in _DEFAULT_CATEGORIES]
    return cats


@dataclass
class Settings:
    # Options -> General
    launch_on_startup: bool = False
    monitor_clipboard: bool = True
    browser_capture: bool = True  # take over browser downloads through the LID extension
    # Options -> File types
    auto_capture_types: str = DEFAULT_FILE_TYPES
    no_auto_sites: str = ""
    # Options -> Downloads
    show_start_dialog: bool = True
    only_add_to_queue: bool = False
    show_complete_dialog: bool = True
    duplicate_action: str = "ask"  # ask | number | overwrite | complete
    verify_checksum: bool = True  # when a download has a checksum, check it once the file is complete
    checksum_from_clipboard: bool = False  # fill the File Info checksum field from a hash on the clipboard
    checksum_mismatch: str = "ask"  # ask | keep | redownload
    user_agent: str = DEFAULT_USER_AGENT
    # Options -> Connection
    max_connections: int = 8
    server_limits: dict[str, int] = field(default_factory=dict)
    retries: int = 10
    retry_delay: float = 3.0
    timeout: float = 30.0
    # Options -> Save to
    categories: list[Category] = field(default_factory=default_categories)
    temp_dir: str = field(default_factory=lambda: str(cache_dir() / "parts"))
    # Options -> Proxy
    proxy: str = ""
    # Speed limiter (Downloads menu)
    speed_limiter_on: bool = False
    speed_limit_kbps: int = 200
    speed_limiter_on_startup: bool = False
    # View / Options -> Appearance
    columns: list[list] = field(default_factory=lambda: [[k, v, w] for k, _, v, w in DEFAULT_COLUMNS])
    theme: str = "ember"  # ember | ember-dark | dark, used when not following the system
    follow_system: bool = True
    dark_theme: str = "ember-dark"  # the theme used when following a dark system
    density: str = "comfortable"  # comfortable | compact
    toolbar_labels: bool = True
    menubar: str = "native"  # native (desktop global menu when there is one) | custom (in the window) | both
    show_categories: bool = True
    show_details: bool = True
    show_statusbar: bool = True
    tray_icon: bool = True
    close_to_tray: bool = True
    start_minimized: bool = False
    list_font: str = ""
    sort_key: str = "date_added"
    sort_desc: bool = False
    window_geometry: str = ""
    sidebar_width: int = 236
    details_height: int = 210
    # Search (last query and options)
    search_flags: int = 0x04
    search_string: str = ""
    # Progress dialog
    show_progress_dialog: bool = True
    # YouTube: the variant picked last time
    media_kind: str = "video"
    media_quality: str = ""

    def category(self, name: str) -> Category:
        for c in self.categories:
            if c.name == name:
                return c
        return self.categories[0]

    def connections_for(self, host: str) -> int:
        from fnmatch import fnmatch

        for pattern, n in self.server_limits.items():
            if fnmatch(host.lower(), pattern.lower()):
                return n
        return self.max_connections


def _from_dict(d: dict) -> Settings:
    s = Settings()
    names = {f.name for f in fields(Settings)}
    # Settings written before the themes existed.
    if "theme" not in d and d.get("dark_mode"):
        d = {**d, "theme": "ember-dark", "follow_system": False}
    if "toolbar_labels" not in d and "toolbar_large" in d:
        d = {**d, "toolbar_labels": bool(d["toolbar_large"])}
    for k, v in d.items():
        if k not in names:
            continue
        if k == "categories":
            v = [Category(**c) for c in v]
        setattr(s, k, v)
    # Columns added in a newer version must still appear.
    have = {c[0] for c in s.columns}
    s.columns += [[k, v, w] for k, _, v, w in DEFAULT_COLUMNS if k not in have]
    return s


def load(path: Path | None = None) -> Settings:
    path = path or config_dir() / "settings.json"
    try:
        return _from_dict(json.loads(path.read_text(encoding="utf-8")))
    except (OSError, ValueError, TypeError):
        return Settings()


def save(settings: Settings, path: Path | None = None) -> None:
    path = path or config_dir() / "settings.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(asdict(settings), indent=2), encoding="utf-8")
    tmp.replace(path)
