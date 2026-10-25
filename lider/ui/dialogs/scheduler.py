"""Scheduler with "Schedule" and "Files in the queue" tabs."""

from __future__ import annotations

from datetime import datetime, timedelta

from PySide6.QtCore import QDate, QSize, Qt, QTime, Signal
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QAbstractItemView,
    QButtonGroup,
    QCheckBox,
    QDateEdit,
    QDialog,
    QFileDialog,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QRadioButton,
    QSpinBox,
    QStackedWidget,
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
from ..icons import bind, category_icon, icon
from ..util import separator
from ..widgets import (COUNT_ROLE, PillDelegate, TreeDelegate, caption, form_label, mono_font, pill_label, primary,
                       set_pill, shell, small_header, tab_strip)

DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]


def _right(left: QWidget, right: QWidget | None = None, indent: int = 0) -> QHBoxLayout:
    """A schedule line: a check box or radio button, its editor pushed to the right edge."""
    row = QHBoxLayout()
    row.setContentsMargins(indent, 0, 0, 0)
    row.addWidget(left)
    row.addStretch(1)
    if right is not None:
        row.addWidget(right)
    return row


class ScheduleTab(QWidget):
    changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(16, 14, 16, 14)
        lay.setSpacing(10)
        self.one_time = QRadioButton("One-time downloading")
        self.periodic = QRadioButton("Periodic synchronization")
        mode = QButtonGroup(self)
        mode.addButton(self.one_time)
        mode.addButton(self.periodic)
        row = QHBoxLayout()
        row.setSpacing(24)
        row.addWidget(self.one_time)
        row.addWidget(self.periodic)
        row.addStretch(1)
        lay.addLayout(row)
        lay.addWidget(separator())

        self.on_startup = QCheckBox("Start download on LID startup")
        lay.addWidget(self.on_startup)
        self.start_enabled = QCheckBox("Start download at")
        self.start_time = self._time_edit()
        lay.addLayout(_right(self.start_enabled, self.start_time))
        self.once = QRadioButton("Once at")
        self.daily = QRadioButton("Daily")
        when = QButtonGroup(self)
        when.addButton(self.once)
        when.addButton(self.daily)
        self.once_date = QDateEdit()
        self.once_date.setCalendarPopup(True)
        self.once_date.setDisplayFormat("yyyy-MM-dd")
        self.once_date.setFont(mono_font(self.once_date.font()))
        self.once_date.setFixedWidth(130)
        lay.addLayout(_right(self.once, self.once_date, indent=26))
        lay.addLayout(_right(self.daily, indent=26))
        days = QGridLayout()
        days.setContentsMargins(52, 0, 0, 0)
        days.setHorizontalSpacing(24)
        days.setVerticalSpacing(8)
        self.day_boxes = []
        for i, name in enumerate(DAYS):
            cb = QCheckBox(name)
            self.day_boxes.append(cb)
            days.addWidget(cb, i % 3, i // 3)
        days.setColumnStretch(3, 1)
        lay.addLayout(days)

        self.every = QWidget()
        er = QHBoxLayout(self.every)
        er.setContentsMargins(0, 0, 0, 0)
        self.every_label = QLabel("Start again every")
        self.every_h = QSpinBox()
        self.every_h.setRange(0, 999)
        self.every_m = QSpinBox()
        self.every_m.setRange(0, 59)
        er.addWidget(self.every_label)
        er.addStretch(1)
        for sb, unit in ((self.every_h, "hours"), (self.every_m, "min")):
            sb.setFont(mono_font(sb.font()))
            sb.setFixedWidth(70)
            er.addWidget(sb)
            er.addWidget(form_label(unit))
        lay.addWidget(self.every)

        lay.addWidget(separator())
        self.stop_enabled = QCheckBox("Stop download at")
        self.stop_time = self._time_edit()
        lay.addLayout(_right(self.stop_enabled, self.stop_time))
        self.retries_enabled = QCheckBox("Number of retries for each file if downloading failed")
        self.retries = QSpinBox()
        self.retries.setRange(0, 999)
        self.retries.setButtonSymbols(QSpinBox.NoButtons)
        self.retries.setFont(mono_font(self.retries.font()))
        self.retries.setFixedWidth(70)
        lay.addLayout(_right(self.retries_enabled, self.retries))
        lay.addWidget(separator())
        self.open_enabled = QCheckBox("Open the following file when done:")
        self.open_file = QLineEdit()
        self.open_file.setFont(mono_font(self.open_file.font()))
        browse = QPushButton("...")
        browse.setObjectName("small")
        browse.setFixedWidth(34)
        browse.clicked.connect(self._browse)
        lay.addWidget(self.open_enabled)
        self.open_row = QWidget()
        orow = QHBoxLayout(self.open_row)
        orow.setContentsMargins(26, 0, 0, 0)
        orow.addWidget(self.open_file, 1)
        orow.addWidget(browse)
        lay.addWidget(self.open_row)
        self.exit_done = QCheckBox("Exit LID when done")
        self.shutdown_done = QCheckBox("Turn off computer when done")
        lay.addWidget(self.exit_done)
        lay.addWidget(self.shutdown_done)
        lay.addStretch(1)
        self.next_run_box = QFrame()
        self.next_run_box.setObjectName("nextRun")
        nl = QHBoxLayout(self.next_run_box)
        nl.setContentsMargins(12, 9, 12, 9)
        nl.setSpacing(8)
        clock = QLabel()
        clock.setPixmap(icon("clock").pixmap(14, 14))
        self.next_run = QLabel()
        nl.addWidget(clock)
        nl.addWidget(self.next_run, 1)
        lay.addWidget(self.next_run_box)

        self.periodic.toggled.connect(self._mode)
        self.daily.toggled.connect(self._mode)
        self.start_enabled.toggled.connect(self._mode)
        self.open_enabled.toggled.connect(self._mode)
        for w in (self.start_enabled, self.once, self.daily, *self.day_boxes):
            w.toggled.connect(self.changed)
        self.start_time.timeChanged.connect(self.changed)
        self.once_date.dateChanged.connect(self.changed)

    def _time_edit(self) -> QTimeEdit:
        t = QTimeEdit()
        t.setDisplayFormat("HH:mm")
        t.setFont(mono_font(t.font()))
        t.setFixedWidth(92)
        return t

    def _browse(self):
        path, _ = QFileDialog.getOpenFileName(self, "Open the following file when done", self.open_file.text())
        if path:
            self.open_file.setText(path)
            self.open_enabled.setChecked(True)

    def _mode(self):
        self.every.setVisible(self.periodic.isChecked())
        self.open_row.setVisible(self.open_enabled.isChecked())
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
        lay.setContentsMargins(16, 14, 16, 14)
        lay.setSpacing(12)
        row = QHBoxLayout()
        self.parallel = QSpinBox()
        self.parallel.setRange(1, 64)
        self.parallel.setFont(mono_font(self.parallel.font()))
        self.parallel.setFixedWidth(64)
        row.addWidget(QLabel("Download"))
        row.addWidget(self.parallel)
        row.addWidget(QLabel("files at the same time"))
        row.addStretch(1)
        lay.addLayout(row)
        self.list = QTreeWidget()
        self.list.setHeaderLabels(["FILE NAME", "SIZE", "STATUS"])
        self.list.setRootIsDecorated(False)
        self.list.setAlternatingRowColors(True)
        self.list.setSelectionMode(QAbstractItemView.SingleSelection)
        self.list.setItemDelegateForColumn(2, PillDelegate({"Queued": ("muted", "Queued")}, self.list))
        self.list.header().setStretchLastSection(False)
        self.list.header().setSectionResizeMode(0, QHeaderView.Stretch)
        self.list.setColumnWidth(1, 90)
        self.list.setColumnWidth(2, 110)
        small_header(self.list)
        lay.addWidget(self.list, 1)
        buttons = QHBoxLayout()
        buttons.setSpacing(6)
        for name, tip, slot in (("up", "Move up", lambda: self._move(-1)),
                                ("down-arrow", "Move down", lambda: self._move(1)),
                                ("remove", "Delete from queue", self._remove)):
            b = QToolButton()
            b.setObjectName("roundBtn")
            bind(b, name)
            b.setIconSize(QSize(14, 14))
            b.setFixedSize(30, 30)
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
        mono = mono_font(self.list.font())
        for d in self.manager.queue_items(self.queue):
            item = QTreeWidgetItem([d.filename, fmt.size(d.size), fmt.status(d) or "Queued"])
            item.setIcon(0, icon(category_icon(d.category)))
            item.setFont(1, mono)
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
        self.resize(820, 720)
        body, footer = shell(self, spacing=0)

        left = QVBoxLayout()
        left.setSpacing(8)
        left.addWidget(caption("Queues"))
        self.tree = QTreeWidget()
        self.tree.setObjectName("queues")
        self.tree.setItemDelegate(TreeDelegate(self.tree))
        self.tree.setHeaderHidden(True)
        self.tree.setRootIsDecorated(False)
        self.tree.setIndentation(0)
        left.addWidget(self.tree, 1)
        qb = QHBoxLayout()
        qb.setSpacing(6)
        for text, slot in (("&New queue", self._new), ("&Delete", self._delete)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            qb.addWidget(b)
        qb.addStretch(1)
        left.addLayout(qb)
        lw = QWidget()
        lw.setLayout(left)
        lw.setFixedWidth(210)
        left.setContentsMargins(0, 0, 0, 0)

        right = QVBoxLayout()
        right.setSpacing(10)
        head = QHBoxLayout()
        head.setSpacing(10)
        self.title = QLabel()
        f = QFont(self.title.font())
        f.setPointSizeF(f.pointSizeF() * 14 / 12)
        f.setBold(True)
        self.title.setFont(f)
        self.state = pill_label("Stopped", "muted")
        head.addWidget(self.title)
        head.addWidget(self.state)
        head.addStretch(1)
        right.addLayout(head)
        card = QFrame()
        card.setObjectName("panel")
        cl = QVBoxLayout(card)
        cl.setContentsMargins(0, 0, 0, 0)
        cl.setSpacing(0)
        strip, self.tabs = tab_strip(["Schedule", "Files in the queue"])
        self.schedule_tab = ScheduleTab()
        self.files_tab = FilesTab(manager)
        self.pages = QStackedWidget()
        self.pages.addWidget(self.schedule_tab)
        self.pages.addWidget(self.files_tab)
        self.tabs.currentChanged.connect(self.pages.setCurrentIndex)
        cl.addWidget(strip)
        cl.addWidget(self.pages, 1)
        right.addWidget(card, 1)

        cols = QHBoxLayout()
        cols.setSpacing(14)
        cols.addWidget(lw)
        cols.addLayout(right, 1)
        body.addLayout(cols, 1)

        start = primary(QPushButton("&Start now"))
        start.setIcon(icon("play-white"))
        start.setIconSize(QSize(13, 13))
        start.clicked.connect(self._start)
        stop = QPushButton("S&top")
        stop.clicked.connect(self._stop)
        footer.addWidget(start)
        footer.addWidget(stop)
        footer.addStretch(1)
        for text, slot in (("&Apply", self._apply), ("&Close", self._close)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            footer.addWidget(b)

        self.tree.currentItemChanged.connect(self._selected)
        self.schedule_tab.changed.connect(self._show_next_run)
        self._fill(select)

    def _show_next_run(self) -> None:
        tab = self.schedule_tab
        sched = QueueSchedule()
        tab.store(sched)
        now = datetime.now()
        t = sched.next_start(now)
        tab.next_run_box.setVisible(t is not None)
        if t is None:
            return
        mins = max(0, int((t - now).total_seconds() + 59) // 60)
        h, m = divmod(mins, 60)
        if t.date() == now.date():
            day = "tonight" if t.hour >= 18 else "today"
        elif t.date() == (now + timedelta(days=1)).date():
            day = "tomorrow"
        else:
            day = t.strftime("%a %d %b")
        tab.next_run.setText(f"Next run {day} at {t:%H:%M} · in {f'{h} h ' if h else ''}{m} m")

    def _fill(self, select: str):
        self.tree.blockSignals(True)
        self.tree.clear()
        for name in self._ordered():
            item = QTreeWidgetItem([name])
            item.setIcon(0, icon("queue-sync" if name == SYNC_QUEUE else "queue"))
            item.setData(0, COUNT_ROLE, len(self.manager.queue_items(name)))
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
        self.title.setText(self.current)
        self._show_state(q.running)
        self.schedule_tab.load(q.schedule)
        self.files_tab.load(self.current)
        self._show_next_run()

    def _show_state(self, running: bool) -> None:
        set_pill(self.state, "Running" if running else "Stopped", "accent" if running else "muted")

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
        self._show_state(True)

    def _stop(self):
        self.manager.stop_queue(self.current)
        self._show_state(False)

    def _close(self):
        self._apply()
        self.accept()
