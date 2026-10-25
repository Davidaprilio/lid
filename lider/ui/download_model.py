"""Table model behind the main download list."""

from __future__ import annotations

import re

from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt, Signal

from ..core import fmt
from ..core.manager import DownloadManager
from ..core.models import Download, Status
from ..core.settings import DEFAULT_COLUMNS
from .icons import category_icon, icon
from .widgets import STATUS_ROLE, mono_font

TITLES = {k: t for k, t, _, _ in DEFAULT_COLUMNS}
ID_ROLE = Qt.UserRole + 1
CHECK = "_check"  # the checkbox column, always first; not one of the user's columns
MONO_KEYS = {"size", "time_left", "rate"}


class DownloadModel(QAbstractTableModel):
    checked_changed = Signal()

    def __init__(self, manager: DownloadManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.rows: list[Download] = []
        self.columns: list[str] = []
        self.checked: set[int] = set()
        self._mono = None
        self.set_columns()

    def set_columns(self) -> None:
        self.beginResetModel()
        self.columns = [CHECK] + [k for k, visible, _ in self.manager.settings.columns if visible]
        self.endResetModel()

    def set_rows(self, rows: list[Download]) -> None:
        if [d.id for d in rows] != [d.id for d in self.rows]:
            self.beginResetModel()
            self.rows = rows
            self.endResetModel()
            visible = {d.id for d in rows}
            if not self.checked <= visible:
                self.checked &= visible  # checks only apply to what the list shows
                self.checked_changed.emit()
        elif rows:
            self.rows = rows
            self.dataChanged.emit(self.index(0, 0), self.index(len(rows) - 1, len(self.columns) - 1))

    # Qt API
    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.rows)

    def columnCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.columns)

    # -- checked rows
    def check_state(self) -> Qt.CheckState:
        n = sum(1 for d in self.rows if d.id in self.checked)
        if n == 0:
            return Qt.Unchecked
        return Qt.Checked if n == len(self.rows) else Qt.PartiallyChecked

    def set_all_checked(self, on: bool) -> None:
        self.checked = {d.id for d in self.rows} if on else set()
        self._emit_check_column()

    def toggle_all(self) -> None:
        self.set_all_checked(self.check_state() != Qt.Checked)

    def set_checked(self, ids, on: bool = True) -> None:
        self.checked = (self.checked | set(ids)) if on else (self.checked - set(ids))
        self._emit_check_column()

    def is_checked_row(self, row: int) -> bool:
        return 0 <= row < len(self.rows) and self.rows[row].id in self.checked

    def _emit_check_column(self) -> None:
        if self.rows:
            self.dataChanged.emit(self.index(0, 0), self.index(len(self.rows) - 1, len(self.columns) - 1))
        self.checked_changed.emit()

    # Qt API
    def flags(self, index):
        f = super().flags(index)
        if index.isValid() and self.columns[index.column()] == CHECK:
            f |= Qt.ItemIsUserCheckable
        return f

    def setData(self, index, value, role=Qt.EditRole):
        if role == Qt.CheckStateRole and index.isValid() and self.columns[index.column()] == CHECK:
            d = self.rows[index.row()]
            on = Qt.CheckState(value) == Qt.Checked
            (self.checked.add if on else self.checked.discard)(d.id)
            self.dataChanged.emit(self.index(index.row(), 0), self.index(index.row(), len(self.columns) - 1))
            self.checked_changed.emit()
            return True
        return False

    def headerData(self, section, orientation, role=Qt.DisplayRole):
        if orientation == Qt.Horizontal and role == Qt.DisplayRole and section < len(self.columns):
            key = self.columns[section]
            return "" if key == CHECK else TITLES[key].upper()
        return None

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid():
            return None
        d = self.rows[index.row()]
        key = self.columns[index.column()]
        if role == ID_ROLE:
            return d.id
        if key == CHECK:
            if role == Qt.CheckStateRole:
                return Qt.Checked if d.id in self.checked else Qt.Unchecked
            return None
        if role == Qt.DisplayRole:
            return "" if key == "status" else self.text(d, key)
        if role == STATUS_ROLE and key == "status":
            return self.status_info(d)
        if role == Qt.FontRole and key in MONO_KEYS:
            if self._mono is None:
                self._mono = mono_font()
            return self._mono
        if role == Qt.DecorationRole:
            if key == "filename":
                return icon(category_icon(d.category))
            if key == "queue" and d.queue:
                return icon("queue")
        if role == Qt.ToolTipRole and key == "status" and d.status == Status.ERROR:
            return d.error
        if role == Qt.ToolTipRole and key == "queue" and d.queue:
            return d.queue
        return None

    def status_info(self, d: Download) -> tuple[str, float, str]:
        """What the Status pill shows: (kind, progress, text)."""
        running = self.manager.is_running(d.id)
        pending = (d.status == Status.COMPLETE and d.checksum and not d.checksum_result and d.id in self.manager.tasks
                   and self.manager.settings.verify_checksum)  # finished, the manager starts the check next poll
        if d.status == Status.VERIFYING or pending:
            p = self.manager.verify_progress(d.id) or 0.0
            return "run", p, f"Verifying {p * 100:.0f}%"
        if d.status == Status.COMPLETE:
            if d.checksum_result == "verified":
                return "verified", 1.0, "Verified"
            if d.checksum_result == "mismatch":
                return "mismatch", 1.0, "Checksum mismatch"
            return "done", 1.0, "Complete"
        if d.status == Status.ERROR:
            code = re.match(r"HTTP (\d{3})", d.error or "")
            return "error", d.progress, f"Error {code.group(1)}" if code else "Error"
        if running:
            text = fmt.status(d) if d.status != Status.DOWNLOADING else (f"{d.progress * 100:.1f}%" if d.size > 0 else fmt.size(d.downloaded))
            return "run", d.progress, text
        if d.downloaded > 0:
            return "pause", d.progress, (f"{d.progress * 100:.1f}% · Paused" if d.size > 0 else f"{fmt.size(d.downloaded)} · Paused")
        return "queue", 0.0, "Queued"

    def text(self, d: Download, key: str) -> str:
        if key == "filename":
            return d.filename
        if key == "size":
            return fmt.size(d.size)
        if key == "status":
            return fmt.status(d)
        if key in ("time_left", "rate"):
            task = self.manager.task(d.id)
            if not task or not task.running:
                return ""
            snap_rate = task.meter.rate()
            if key == "rate":
                return fmt.rate(snap_rate)
            if snap_rate > 0 and d.size > 0:
                return fmt.duration(int((d.size - d.downloaded) / snap_rate))
            return ""
        if key == "last_try":
            return fmt.date(d.last_try)
        if key == "date_added":
            return fmt.date(d.date_added)
        if key == "description":
            return d.description
        if key == "save_to":
            return d.path
        if key == "referer":
            return d.referer
        if key == "parent_page":
            return d.parent_page
        return ""


SORT_KEYS = {
    "date_added": lambda d: d.id,
    "filename": lambda d: d.filename.lower(),
    "size": lambda d: d.size,
    "status": lambda d: (d.status.value, d.progress),
    "time_left": lambda d: d.size - d.downloaded,
    "rate": lambda d: d.downloaded,
    "last_try": lambda d: d.last_try,
    "description": lambda d: d.description.lower(),
    "save_to": lambda d: d.save_dir.lower(),
    "referer": lambda d: d.referer,
    "parent_page": lambda d: d.parent_page,
    "queue": lambda d: (d.queue, d.queue_order),
}
