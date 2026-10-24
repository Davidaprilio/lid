"""Scheduler with "Schedule" and "Files in the queue" tabs."""

from __future__ import annotations

from PySide6.QtCore import QDate, Qt, QTime
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QAbstractItemView,
    QButtonGroup,
    QCheckBox,
    QDateEdit,
    QDialog,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QRadioButton,
    QSpinBox,
    QTabWidget,
    QTimeEdit,
    QToolButton,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ...core import fmt
from ...core.manager import DownloadManager, QueueSchedule
from ...core.models import MAIN_QUEUE, SYNC_QUEUE
from ..icons import file_icon, icon
from ..util import dlu, separator
from ..widgets import primary

DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]


class ScheduleTab(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        g = QGridLayout(self)
        self.one_time = QRadioButton("One-time downloading")
        self.periodic = QRadioButton("Periodic synchronization")
        mode = QButtonGroup(self)
        mode.addButton(self.one_time)
        mode.addButton(self.periodic)
        g.addWidget(self.one_time, 0, 0, 1, 2)
        g.addWidget(self.periodic, 0, 2, 1, 2)
        g.addWidget(separator(), 1, 0, 1, 4)
        self.on_startup = QCheckBox("Start download on LID startup")
        g.addWidget(self.on_startup, 2, 0, 1, 4)
        self.start_enabled = QCheckBox("Start download at")
        self.start_time = QTimeEdit()
        self.start_time.setDisplayFormat("HH:mm")
        g.addWidget(self.start_enabled, 3, 0)
        g.addWidget(self.start_time, 3, 1)

        self.once = QRadioButton("Once at")
        self.daily = QRadioButton("Daily")
        when = QButtonGroup(self)
        when.addButton(self.once)
        when.addButton(self.daily)
        self.once_date = QDateEdit()
        self.once_date.setCalendarPopup(True)
        self.once_date.setDisplayFormat("yyyy-MM-dd")
        g.addWidget(self.once, 4, 0)
        g.addWidget(self.once_date, 4, 1)
        g.addWidget(self.daily, 5, 0, Qt.AlignTop)
        days = QGridLayout()
        self.day_boxes = []
        for i, name in enumerate(DAYS):
            cb = QCheckBox(name)
            self.day_boxes.append(cb)
            days.addWidget(cb, i % 3, i // 3)
        g.addLayout(days, 5, 1, 1, 3)

        self.every_label = QLabel("Start again every")
        self.every_h = QSpinBox()
        self.every_h.setRange(0, 999)
        self.every_m = QSpinBox()
        self.every_m.setRange(0, 59)
        row = QHBoxLayout()
        row.addWidget(self.every_h)
        row.addWidget(QLabel("hours"))
        row.addWidget(self.every_m)
        row.addWidget(QLabel("min"))
        row.addStretch(1)
        g.addWidget(self.every_label, 6, 0)
        g.addLayout(row, 6, 1, 1, 3)

        g.addWidget(separator(), 7, 0, 1, 4)
        self.stop_enabled = QCheckBox("Stop download at")
        self.stop_time = QTimeEdit()
        self.stop_time.setDisplayFormat("HH:mm")
        g.addWidget(self.stop_enabled, 8, 0)
        g.addWidget(self.stop_time, 8, 1)
        self.retries_enabled = QCheckBox("Number of retries for each file if downloading failed :")
        self.retries = QSpinBox()
        self.retries.setRange(0, 999)
        g.addWidget(self.retries_enabled, 9, 0, 1, 3)
        g.addWidget(self.retries, 9, 3)
        g.addWidget(separator(), 10, 0, 1, 4)
        self.open_enabled = QCheckBox("Open the following file when done:")
        self.open_file = QLineEdit()
        browse = QPushButton("...")
        browse.setObjectName("small")
        browse.setFixedWidth(34)
        browse.clicked.connect(self._browse)
        g.addWidget(self.open_enabled, 11, 0, 1, 4)
        g.addWidget(self.open_file, 12, 0, 1, 3)
        g.addWidget(browse, 12, 3)
        self.exit_done = QCheckBox("Exit LID when done")
        self.shutdown_done = QCheckBox("Turn off computer when done")
        g.addWidget(self.exit_done, 13, 0, 1, 4)
        g.addWidget(self.shutdown_done, 14, 0, 1, 4)
        g.setRowStretch(15, 1)
        g.setColumnStretch(3, 1)

        self.periodic.toggled.connect(self._mode)
        self.daily.toggled.connect(self._mode)
        self.start_enabled.toggled.connect(self._mode)

    def _browse(self):
        path, _ = QFileDialog.getOpenFileName(self, "Open the following file when done", self.open_file.text())
        if path:
            self.open_file.setText(path)
            self.open_enabled.setChecked(True)

    def _mode(self):
        periodic = self.periodic.isChecked()
        self.every_label.setEnabled(periodic)
        self.every_h.setEnabled(periodic)
        self.every_m.setEnabled(periodic)
        on = self.start_enabled.isChecked()
        self.start_time.setEnabled(on)
        self.once.setEnabled(on)
        self.daily.setEnabled(on)
        self.once_date.setEnabled(on and self.once.isChecked())
        for cb in self.day_boxes:
            cb.setEnabled(on and self.daily.isChecked())

    def load(self, s: QueueSchedule):
        (self.periodic if s.periodic else self.one_time).setChecked(True)
        self.on_startup.setChecked(s.start_on_startup)
        self.start_enabled.setChecked(s.start_enabled)
        self.start_time.setTime(QTime.fromString(s.start_time, "HH:mm"))
        (self.daily if s.daily else self.once).setChecked(True)
        self.once_date.setDate(QDate.fromString(s.once_date, "yyyy-MM-dd") if s.once_date else QDate.currentDate())
        for i, cb in enumerate(self.day_boxes):
            cb.setChecked(i in s.days)
        self.every_h.setValue(s.every_hours)
        self.every_m.setValue(s.every_minutes)
        self.stop_enabled.setChecked(s.stop_enabled)
        self.stop_time.setTime(QTime.fromString(s.stop_time, "HH:mm"))
        self.retries_enabled.setChecked(s.retries_enabled)
        self.retries.setValue(s.retries)
        self.open_enabled.setChecked(bool(s.open_when_done))
        self.open_file.setText(s.open_when_done)
        self.exit_done.setChecked(s.exit_when_done)
        self.shutdown_done.setChecked(s.shutdown_when_done)
        self._mode()

    def store(self, s: QueueSchedule):
        s.periodic = self.periodic.isChecked()
        s.start_on_startup = self.on_startup.isChecked()
        s.start_enabled = self.start_enabled.isChecked()
        s.start_time = self.start_time.time().toString("HH:mm")
        s.daily = self.daily.isChecked()
        s.once_date = "" if s.daily else self.once_date.date().toString("yyyy-MM-dd")
        s.days = [i for i, cb in enumerate(self.day_boxes) if cb.isChecked()]
        s.every_hours, s.every_minutes = self.every_h.value(), self.every_m.value()
        s.stop_enabled = self.stop_enabled.isChecked()
        s.stop_time = self.stop_time.time().toString("HH:mm")
        s.retries_enabled = self.retries_enabled.isChecked()
        s.retries = self.retries.value()
        s.open_when_done = self.open_file.text().strip() if self.open_enabled.isChecked() else ""
        s.exit_when_done = self.exit_done.isChecked()
        s.shutdown_when_done = self.shutdown_done.isChecked()


class FilesTab(QWidget):
    def __init__(self, manager: DownloadManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.queue = ""
        lay = QVBoxLayout(self)
        row = QHBoxLayout()
        self.parallel = QSpinBox()
        self.parallel.setRange(1, 64)
        row.addWidget(QLabel("Download"))
        row.addWidget(self.parallel)
        row.addWidget(QLabel("files at the same time"))
        row.addStretch(1)
        lay.addLayout(row)
        self.list = QTreeWidget()
        self.list.setHeaderLabels(["File Name", "Size", "Status", "Time left"])
        self.list.setRootIsDecorated(False)
        self.list.setSelectionMode(QAbstractItemView.SingleSelection)
        self.list.setColumnWidth(0, 220)
        lay.addWidget(self.list, 1)
        buttons = QHBoxLayout()
        for name, tip, slot in (("up", "Move up", lambda: self._move(-1)),
                                ("down-arrow", "Move down", lambda: self._move(1)),
                                ("remove", "Delete from queue", self._remove)):
            b = QToolButton()
            b.setIcon(icon(name))
            b.setToolTip(tip)
            b.clicked.connect(slot)
            buttons.addWidget(b)
        buttons.addStretch(1)
        lay.addLayout(buttons)

    def load(self, name: str):
        self.queue = name
        self.parallel.setValue(self.manager.queues[name].schedule.max_parallel)
        self.fill()

    def fill(self, select_id: int | None = None):
        self.list.clear()
        for d in self.manager.queue_items(self.queue):
            item = QTreeWidgetItem([d.filename, fmt.size(d.size), fmt.status(d), ""])
            item.setIcon(0, file_icon(d.filename))
            item.setData(0, Qt.UserRole, d.id)
            self.list.addTopLevelItem(item)
            if d.id == select_id:
                self.list.setCurrentItem(item)

    def _current_id(self) -> int | None:
        item = self.list.currentItem()
        return item.data(0, Qt.UserRole) if item else None

    def _move(self, delta):
        i = self._current_id()
        if i is not None:
            self.manager.move_in_queue(i, delta)
            self.fill(i)

    def _remove(self):
        i = self._current_id()
        if i is not None:
            self.manager.set_queue([i], "")
            self.fill()


class SchedulerDialog(QDialog):
    def __init__(self, manager: DownloadManager, parent=None, select: str = MAIN_QUEUE, on_events=None):
        super().__init__(parent)
        self.manager = manager
        self.on_events = on_events
        self.current = ""
        self.setWindowTitle("Scheduler")
        self.resize(dlu(434, 286))

        left = QVBoxLayout()
        head = QHBoxLayout()
        pic = QLabel()
        pic.setPixmap(icon("queue").pixmap(24, 24))
        head.addWidget(pic)
        head.addWidget(QLabel("Queues"))
        head.addStretch(1)
        left.addLayout(head)
        self.tree = QTreeWidget()
        self.tree.setHeaderHidden(True)
        self.tree.setRootIsDecorated(False)
        left.addWidget(self.tree, 1)

        right = QVBoxLayout()
        self.title = QLabel()
        f = QFont(self.title.font())
        f.setBold(True)
        self.title.setFont(f)
        right.addWidget(self.title)
        self.tabs = QTabWidget()
        self.schedule_tab = ScheduleTab()
        self.files_tab = FilesTab(manager)
        self.tabs.addTab(self.schedule_tab, "Schedule")
        self.tabs.addTab(self.files_tab, "Files in the queue")
        right.addWidget(self.tabs, 1)

        body = QHBoxLayout()
        lw = QWidget()
        lw.setLayout(left)
        lw.setFixedWidth(dlu(122, 0).width())
        body.addWidget(lw)
        body.addLayout(right, 1)

        buttons = QHBoxLayout()
        for text, slot in (("&New queue", self._new), ("&Delete", self._delete)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            buttons.addWidget(b)
        buttons.addSpacing(20)
        for text, slot in (("&Start now", self._start), ("S&top", self._stop)):
            b = primary(QPushButton(text)) if slot == self._start else QPushButton(text)
            b.clicked.connect(slot)
            buttons.addWidget(b)
        buttons.addStretch(1)
        for text, slot in (("&Apply", self._apply), ("&Close", self._close)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            buttons.addWidget(b)

        lay = QVBoxLayout(self)
        lay.addLayout(body, 1)
        lay.addLayout(buttons)

        self.tree.currentItemChanged.connect(self._selected)
        self._fill(select)

    def _fill(self, select: str):
        self.tree.blockSignals(True)
        self.tree.clear()
        for name in self._ordered():
            item = QTreeWidgetItem([name])
            item.setIcon(0, icon("queue-sync" if name == SYNC_QUEUE else "queue"))
            self.tree.addTopLevelItem(item)
        self.tree.blockSignals(False)
        names = self._ordered()
        idx = names.index(select) if select in names else 0
        self.tree.setCurrentItem(self.tree.topLevelItem(idx))

    def _ordered(self) -> list[str]:
        others = sorted(n for n in self.manager.queues if n not in (MAIN_QUEUE, SYNC_QUEUE))
        return [MAIN_QUEUE, SYNC_QUEUE] + others

    def _selected(self, item, _prev):
        if self.current:
            self._apply()
        if not item:
            return
        self.current = item.text(0)
        q = self.manager.queues[self.current]
        state = "running" if q.running else "stopped"
        self.title.setText(f"{self.current}  ({state})")
        self.schedule_tab.load(q.schedule)
        self.files_tab.load(self.current)

    def _apply(self):
        if not self.current or self.current not in self.manager.queues:
            return
        q = self.manager.queues[self.current]
        self.schedule_tab.store(q.schedule)
        q.schedule.max_parallel = self.files_tab.parallel.value()
        self.manager.save_queue(self.current)

    def _new(self):
        name, ok = QInputDialog.getText(self, "Enter queue name",
                                        "Enter a name for the new queue that will be displayed in the list of queues")
        name = name.strip()
        if ok and name and name not in self.manager.queues:
            self._apply()
            self.manager.add_queue(name)
            self.current = ""
            self._fill(name)

    def _delete(self):
        if self.current in (MAIN_QUEUE, SYNC_QUEUE):
            QMessageBox.information(self, "Scheduler", "The default queues cannot be deleted.")
            return
        if QMessageBox.question(self, "Scheduler", f'Delete queue "{self.current}"?') == QMessageBox.Yes:
            self.manager.delete_queue(self.current)
            self.current = ""
            self._fill(MAIN_QUEUE)

    def _start(self):
        self._apply()
        events = self.manager.start_queue(self.current)
        if self.on_events:
            self.on_events(events)
        self.title.setText(f"{self.current}  (running)")

    def _stop(self):
        self.manager.stop_queue(self.current)
        self.title.setText(f"{self.current}  (stopped)")

    def _close(self):
        self._apply()
        self.accept()
