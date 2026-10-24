"""LID's own icon set ("Signal" style, designed in the LID Icon Set canvas).

The SVGs live in ``assets/icons``. They are drawn with one ink colour and one
accent colour that the current theme swaps in; widgets registered with ``bind``
are re-iconed when the theme changes. Names missing from the set fall back to
the desktop theme, then to Qt's built-in icons.
"""

from __future__ import annotations

import weakref
from functools import lru_cache
from pathlib import Path

import shiboken6
from PySide6.QtCore import QByteArray, QFileInfo, QRectF, Qt
from PySide6.QtGui import QIcon, QPainter, QPixmap
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtWidgets import QApplication, QFileIconProvider, QStyle

ICON_DIR = Path(__file__).parent / "assets" / "icons"
INK = "#1E293B"  # ink and accent as drawn in the SVG files
ACCENT = "#2563EB"
SIZES = (16, 20, 24, 32, 48, 64)

ALIASES = {
    "download": "folder-download",
    "downloads": "folder-download",
    "folder-open": "folder",
}

_ink, _accent = INK, ACCENT
_bound: list[tuple[weakref.ref, str]] = []


def set_colors(ink: str, accent: str) -> None:
    """Switch ink and accent colour and refresh every widget/action registered with ``bind``."""
    global _ink, _accent
    if (ink, accent) == (_ink, _accent):
        return
    _ink, _accent = ink, accent
    icon.cache_clear()
    alive = []
    for ref, name in _bound:
        obj = ref()
        if obj is not None and shiboken6.isValid(obj):  # the C++ side of a closed window may be gone already
            obj.setIcon(icon(name))
            alive.append((ref, name))
    _bound[:] = alive


def bind(obj, name: str):
    """Set ``obj``'s icon and keep it in sync with dark mode. Returns ``obj``."""
    obj.setIcon(icon(name))
    _bound[:] = [(ref, n) for ref, n in _bound if ref() is not obj]  # re-binding replaces the name
    _bound.append((weakref.ref(obj), name))
    return obj


def has(name: str) -> bool:
    return (ICON_DIR / f"{ALIASES.get(name, name)}.svg").exists()


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
        svg = svg.replace(INK.encode(), _ink.encode()).replace(ACCENT.encode(), _accent.encode())
        app = QApplication.instance()
        ratio = app.devicePixelRatio() if app else 1.0
        ic = QIcon()
        for size in SIZES:
            ic.addPixmap(_render(svg, size, ratio))
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
