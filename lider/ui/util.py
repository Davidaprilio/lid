"""Small Qt helpers shared by the windows and dialogs."""

from __future__ import annotations

import os
import subprocess
import threading
from typing import Any, Callable

from PySide6.QtCore import QObject, QSize, QUrl, Signal, Slot
from PySide6.QtGui import QDesktopServices, QFont
from PySide6.QtWidgets import QFrame, QHBoxLayout, QLabel, QWidget

from .icons import icon


def dlu(w: int, h: int) -> QSize:
    """Convert Windows dialog units (MS Shell Dlg 8pt) to pixels."""
    return QSize(round(w * 1.5), round(h * 1.625))


class _Relay(QObject):
    finished = Signal(object, object)

    def __init__(self, callback: Callable[[Any, BaseException | None], None]):
        super().__init__()
        self._callback = callback
        self.finished.connect(self._deliver)

    @Slot(object, object)
    def _deliver(self, result, error):
        try:
            self._callback(result, error)
        finally:
            _pending.discard(self)
            self.deleteLater()


_pending: set[_Relay] = set()


def run_async(fn: Callable[[], Any], callback: Callable[[Any, BaseException | None], None]) -> None:
    """Run ``fn`` on a worker thread and call ``callback(result, error)`` on the UI thread."""
    relay = _Relay(callback)
    _pending.add(relay)

    def work():
        try:
            relay.finished.emit(fn(), None)
        except Exception as e:  # noqa: BLE001 - reported to the UI
            relay.finished.emit(None, e)

    threading.Thread(target=work, daemon=True).start()


def open_path(path: str) -> None:
    QDesktopServices.openUrl(QUrl.fromLocalFile(path))


def open_folder(path: str) -> None:
    """Open the containing folder and select the file when the file manager supports it."""
    try:
        subprocess.Popen(
            ["dbus-send", "--session", "--dest=org.freedesktop.FileManager1", "--type=method_call",
             "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems",
             f"array:string:{QUrl.fromLocalFile(path).toString()}", "string:"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
    except OSError:
        open_path(os.path.dirname(path))


def separator() -> QFrame:
    line = QFrame()
    line.setFrameShape(QFrame.HLine)
    line.setFrameShadow(QFrame.Sunken)
    return line


def page_header(icon_name: str, title: str) -> QWidget:
    """Icon + bold title + rule, the header of every Options page."""
    w = QWidget()
    lay = QHBoxLayout(w)
    lay.setContentsMargins(0, 0, 0, 4)
    lay.setSpacing(10)
    pic = QLabel()
    pic.setPixmap(icon(icon_name).pixmap(22, 22))
    text = QLabel(title)
    f = QFont(text.font())
    f.setBold(True)
    f.setPointSizeF(f.pointSizeF() * 14 / 12)
    text.setFont(f)
    lay.addWidget(pic)
    lay.addWidget(text, 1)
    return w
