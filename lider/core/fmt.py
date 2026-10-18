"""Text formats used in the download list and dialogs."""

from __future__ import annotations

import time

from .models import Download, Status


def size(n: int) -> str:
    if n is None or n < 0:
        return ""
    if n < 1024:
        return f"{n} Bytes"
    for unit in ("KB", "MB", "GB", "TB"):
        n /= 1024
        if n < 1024 or unit == "TB":
            return f"{n:.2f} {unit}"
    return ""


def rate(bps: float) -> str:
    return f"{size(int(bps))}/sec" if bps > 0 else ""


def duration(seconds: int) -> str:
    if seconds is None or seconds < 0:
        return ""
    h, rem = divmod(int(seconds), 3600)
    m, s = divmod(rem, 60)
    if h:
        return f"{h} hour(s) {m} min"
    if m:
        return f"{m} min {s} sec"
    return f"{s} sec"


def date(ts: float) -> str:
    return time.strftime("%b %d %H:%M:%S %Y", time.localtime(ts)) if ts else ""


def status(d: Download) -> str:
    if d.status == Status.COMPLETE:
        return "Complete"
    if d.status == Status.ERROR:
        return "Error"
    if d.status == Status.CONNECTING:
        return "Connecting..."
    if d.status == Status.ASSEMBLING:
        return "Assembling..."
    if d.status == Status.QUEUED and d.downloaded == 0:
        return ""
    if d.size > 0:
        return f"{100 * d.downloaded / d.size:.2f}%"
    return size(d.downloaded)
