"""Pick a category for a new download from its site and file type."""

from __future__ import annotations

import os
from fnmatch import fnmatch
from urllib.parse import urlsplit

from .settings import Category, Settings


def extension(filename: str) -> str:
    return os.path.splitext(filename)[1].lstrip(".").lower()


def _site_matches(cat: Category, host: str) -> bool:
    return any(fnmatch(host, p.lower()) for p in cat.sites)


def classify(settings: Settings, filename: str, url: str = "") -> Category:
    """Site rules win over file-type rules; anything else goes to "General"."""
    host = (urlsplit(url).hostname or "").lower()
    if host:
        for cat in settings.categories:
            if cat.sites and _site_matches(cat, host):
                return cat
    ext = extension(filename)
    if ext:
        for cat in settings.categories:
            if any(fnmatch(ext, p.lower()) for p in cat.extensions):
                return cat
    return settings.categories[0]


def is_auto_capture_type(settings: Settings, filename: str) -> bool:
    ext = extension(filename)
    return bool(ext) and any(fnmatch(ext, p.lower()) for p in settings.auto_capture_types.split())
