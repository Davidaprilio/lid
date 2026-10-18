"""Table model behind the main download list."""

from __future__ import annotations

from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt

from ..core import fmt
from ..core.manager import DownloadManager
from ..core.models import Download, Status
from ..core.settings import DEFAULT_COLUMNS
from .icons import file_icon, icon

TITLES = {k: t for k, t, _, _ in DEFAULT_COLUMNS}
ID_ROLE = Qt.UserRole + 1


class DownloadModel(QAbstractTableModel):
    def __init__(self, manager: DownloadManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.rows: list[Download] = []
        self.columns: list[str] = []
        self.set_columns()

    def set_columns(self) -> None:
        self.beginResetModel()
        self.columns = [k for k, visible, _ in self.manager.settings.columns if visible]
        self.endResetModel()

    def set_rows(self, rows: list[Download]) -> None:
        if [d.id for d in rows] != [d.id for d in self.rows]:
            self.beginResetModel()
            self.rows = rows
            self.endResetModel()
        elif rows:
            self.rows = rows
            self.dataChanged.emit(self.index(0, 0), self.index(len(rows) - 1, len(self.columns) - 1))

    # Qt API
    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.rows)

    def columnCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.columns)

    def headerData(self, section, orientation, role=Qt.DisplayRole):
        if orientation == Qt.Horizontal and role == Qt.DisplayRole and section < len(self.columns):
            return TITLES[self.columns[section]]
        return None

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid():
            return None
        d = self.rows[index.row()]
        key = self.columns[index.column()]
        if role == ID_ROLE:
            return d.id
        if role == Qt.DisplayRole:
            return self.text(d, key)
        if role == Qt.DecorationRole:
            if key == "filename":
                return file_icon(d.filename)
            if key == "queue" and d.queue:
                return icon("queue")
        if role == Qt.ToolTipRole and key == "status" and d.status == Status.ERROR:
            return d.error
        if role == Qt.ToolTipRole and key == "queue" and d.queue:
            return d.queue
        return None

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
