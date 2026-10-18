"""LID's own icon set ("Signal" style, designed in the LID Icon Set canvas).

The SVGs live in ``assets/icons``. They are drawn with one ink colour that is
swapped for a light one in dark mode; widgets registered with ``bind`` are
re-iconed when the mode changes. Names missing from the set fall back to the
desktop theme, then to Qt's built-in icons.
"""

from __future__ import annotations

import re
import weakref
from functools import lru_cache
from pathlib import Path

from PySide6.QtCore import QByteArray, QFileInfo, QRectF, Qt
from PySide6.QtGui import QIcon, QPainter, QPixmap
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtWidgets import QApplication, QFileIconProvider, QStyle

ICON_DIR = Path(__file__).parent / "assets" / "icons"
INK_LIGHT = "#1E293B"
INK_DARK = "#E2E8F0"
SIZES = (16, 20, 24, 32, 48, 64)

ALIASES = {
    "download": "folder-download",
    "downloads": "folder-download",
    "folder-open": "folder",
}

_dark = False
_bound: list[tuple[weakref.ref, str]] = []


def set_dark(on: bool) -> None:
    """Switch ink colour and refresh every widget/action registered with ``bind``."""
    global _dark
    if on == _dark:
        return
    _dark = on
    icon.cache_clear()
    alive = []
    for ref, name in _bound:
        obj = ref()
        if obj is not None:
            obj.setIcon(icon(name))
            alive.append((ref, name))
    _bound[:] = alive


def bind(obj, name: str):
    """Set ``obj``'s icon and keep it in sync with dark mode. Returns ``obj``."""
    obj.setIcon(icon(name))
    _bound.append((weakref.ref(obj), name))
    return obj


def has(name: str) -> bool:
    return (ICON_DIR / f"{ALIASES.get(name, name)}.svg").exists()


def _selected_variant(svg: bytes) -> bytes:
    """White glyph for highlighted rows; white knock-outs inside badges turn dark blue."""
    text = svg.decode().replace("#FFFFFF", "@KNOCKOUT@")
    text = re.sub(r"#[0-9A-Fa-f]{6}", "#FFFFFF", text)
    return text.replace("@KNOCKOUT@", "#1D4ED8").encode()


def _render(svg: bytes, size: int, ratio: float) -> QPixmap:
    renderer = QSvgRenderer(QByteArray(svg))
    px = round(size * ratio)
    pm = QPixmap(px, px)
    pm.fill(Qt.transparent)
    p = QPainter(pm)
    p.setRenderHint(QPainter.Antialiasing)
    renderer.render(p, QRectF(0, 0, px, px))
    p.end()
    pm.setDevicePixelRatio(ratio)
    return pm


@lru_cache(maxsize=None)
def icon(name: str) -> QIcon:
    path = ICON_DIR / f"{ALIASES.get(name, name)}.svg"
    if path.exists():
        svg = path.read_bytes()
        if _dark:
            svg = svg.replace(INK_LIGHT.encode(), INK_DARK.encode())
        app = QApplication.instance()
        ratio = app.devicePixelRatio() if app else 1.0
        selected = _selected_variant(svg)
        ic = QIcon()
        for size in SIZES:
            ic.addPixmap(_render(svg, size, ratio))
            ic.addPixmap(_render(selected, size, ratio), QIcon.Selected)
        return ic
    themed = QIcon.fromTheme(name)
    if not themed.isNull():
        return themed
    return QApplication.style().standardIcon(QStyle.SP_FileIcon)


def category_icon(name: str) -> str:
    key = f"category-{name.lower()}"
    return key if has(key) else "folder"


_provider: QFileIconProvider | None = None


@lru_cache(maxsize=512)
def file_icon(filename: str) -> QIcon:
    global _provider
    if _provider is None:
        _provider = QFileIconProvider()
    ic = _provider.icon(QFileInfo(filename))
    return ic if not ic.isNull() else QApplication.style().standardIcon(QStyle.SP_FileIcon)
