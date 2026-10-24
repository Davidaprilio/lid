"""Download progress (status, speed limiter, options on completion) and Download complete."""

from __future__ import annotations

import os
import shlex
import subprocess

from PySide6.QtCore import QRectF, Qt, QTimer
from PySide6.QtGui import QColor, QPainter, QPainterPath
from PySide6.QtWidgets import (
    QCheckBox,
    QDialog,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QProgressBar,
    QPushButton,
    QSpinBox,
    QTabWidget,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ...core import fmt
from ...core.manager import DownloadManager
from ...core.models import Download, Segment, Status
from ..icons import file_icon
from ..util import dlu, open_folder, open_path, separator


class SegmentBar(QWidget):
    """Draws each connection's segment: downloaded part filled, the rest empty."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.size_total = 0
        self.segments: list[Segment] = []
        self.setMinimumHeight(12)
        self.setMaximumHeight(14)

    def set_segments(self, size: int, segments: list[Segment]) -> None:
        self.size_total, self.segments = size, segments
        self.update()

    def paintEvent(self, _event):
        from .. import theme

        c = theme.colors()
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        r = QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5)
        clip = QPainterPath()
        clip.addRoundedRect(r, 4, 4)
        p.fillPath(clip, QColor(c["track"]))
        p.setClipPath(clip)
        if self.size_total > 0:
            scale = r.width() / self.size_total
            for s in self.segments:
                end = s.pos if s.end < 0 else min(s.pos, s.end)
                done = s.end >= 0 and s.pos >= s.end
                p.fillRect(QRectF(r.left() + s.start * scale, r.top(), max(1.0, (end - s.start) * scale), r.height()),
                           QColor(c["ok"] if done else c["ac"]))
            gap = QColor(c["pane"])
            for s in self.segments[1:]:
                p.fillRect(QRectF(r.left() + s.start * scale - 1, r.top(), 2, r.height()), gap)


class ProgressDialog(QDialog):
    def __init__(self, manager: DownloadManager, download_id: int, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.id = download_id
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.setWindowFlag(Qt.WindowMinimizeButtonHint, True)
        self.resize(dlu(353, 250).width(), 0)
        d = self.d

        # -- Download status tab (363)
        status_tab = QWidget()
        g = QGridLayout(status_tab)
        variant = (d.media or {}).get("variant")
        self.url_label = QLabel(f"{d.url}   ({variant['quality']} {variant['ext'].upper()})" if variant else d.url)
        self.url_label.setTextInteractionFlags(Qt.TextSelectableByMouse)
        self.url_label.setMinimumWidth(1)
        self.status = QLabel("Connecting...")
        self.file_size = QLabel("(unknown)")
        self.downloaded = QLabel("0 Bytes")
        self.rate = QLabel("")
        self.time_left = QLabel("(unknown)")
        self.resume = QLabel("(unknown)")
        g.addWidget(self.url_label, 0, 0, 1, 2)
        for row, (title, lab) in enumerate((("Status", self.status), ("File size", self.file_size),
                                            ("Downloaded", self.downloaded), ("Transfer rate", self.rate),
                                            ("Time left", self.time_left), ("Resume capability", self.resume)), 1):
            g.addWidget(QLabel(title), row, 0)
            g.addWidget(lab, row, 1)
        g.setColumnStretch(1, 1)

        # -- Speed Limiter tab (365)
        limit_tab = QWidget()
        lg = QGridLayout(limit_tab)
        self.rate2 = QLabel("")
        self.use_limit = QCheckBox("Use Speed Limiter")
        self.use_limit.setChecked(d.speed_limit_kbps > 0)
        self.limit = QSpinBox()
        self.limit.setRange(1, 10_000_000)
        self.limit.setValue(d.speed_limit_kbps or self.manager.settings.speed_limit_kbps)
        self.remember_limit = QCheckBox("Remember Speed Limiter settings for this file\non download start")
        self.remember_limit.setChecked(True)
        lg.addWidget(QLabel("Transfer rate"), 0, 0)
        lg.addWidget(self.rate2, 0, 1)
        lg.addWidget(self.use_limit, 1, 0, 1, 2)
        lg.addWidget(QLabel("Maximum download speed:"), 2, 0, 1, 2)
        row = QHBoxLayout()
        row.addWidget(self.limit)
        row.addWidget(QLabel("KBytes/sec"))
        row.addStretch(1)
        lg.addLayout(row, 3, 0, 1, 2)
        lg.addWidget(self.remember_limit, 4, 0, 1, 2)
        lg.setColumnStretch(1, 1)
        self.use_limit.toggled.connect(self._apply_limit)
        self.limit.valueChanged.connect(self._apply_limit)

        # -- Options on completion tab (364)
        done_tab = QWidget()
        dg = QVBoxLayout(done_tab)
        self.save_to = QLineEdit(d.path)
        self.save_to.setReadOnly(True)
        self.save_to.setCursorPosition(0)
        self.show_complete = QCheckBox("Show download complete dialog")
        self.show_complete.setChecked(self.manager.settings.show_complete_dialog)
        self.exit_when_done = QCheckBox("Exit LID when done")
        self.shutdown_when_done = QCheckBox("Turn off computer when done")
        dg.addWidget(QLabel("Save To:"))
        dg.addWidget(self.save_to)
        dg.addWidget(self.show_complete)
        dg.addWidget(separator())
        dg.addWidget(QLabel("Power options"))
        dg.addWidget(self.exit_when_done)
        dg.addWidget(self.shutdown_when_done)
        dg.addStretch(1)

        self.tabs = QTabWidget()
        self.tabs.addTab(status_tab, "Download status")
        self.tabs.addTab(limit_tab, "Speed Limiter")
        self.tabs.addTab(done_tab, "Options on completion")

        self.progress = QProgressBar()
        self.progress.setRange(0, 1000)
        self.progress.setTextVisible(False)

        self.details_btn = QPushButton("<< Hide details")
        self.pause_btn = QPushButton("Pause")
        cancel = QPushButton("Cancel")
        self.details_btn.clicked.connect(self._toggle_details)
        self.pause_btn.clicked.connect(self._pause_resume)
        cancel.clicked.connect(self._cancel)
        buttons = QHBoxLayout()
        buttons.addWidget(self.details_btn)
        buttons.addStretch(1)
        buttons.addWidget(self.pause_btn)
        buttons.addWidget(cancel)

        # -- details: segment bar + connection list
        self.details = QWidget()
        dl = QVBoxLayout(self.details)
        dl.setContentsMargins(0, 0, 0, 0)
        dl.addWidget(QLabel("Start positions and download progress by connections"))
        self.bar = SegmentBar()
        dl.addWidget(self.bar)
        self.conns = QTreeWidget()
        self.conns.setHeaderLabels(["N.", "Downloaded", "Speed", "Info"])
        self.conns.setRootIsDecorated(False)
        self.conns.setColumnWidth(0, 40)
        self.conns.setColumnWidth(1, 110)
        self.conns.setColumnWidth(2, 110)
        self.conns.setMinimumHeight(dlu(0, 79).height())
        dl.addWidget(self.conns)

        lay = QVBoxLayout(self)
        lay.addWidget(self.tabs)
        lay.addWidget(self.progress)
        lay.addLayout(buttons)
        lay.addWidget(self.details, 1)

        self.timer = QTimer(self)
        self.timer.timeout.connect(self.refresh)
        self.timer.start(500)
        self.refresh()

    @property
    def d(self) -> Download:
        return self.manager.downloads[self.id]

    def completion_options(self) -> dict:
        return {
            "show_complete": self.show_complete.isChecked(),
            "exit": self.exit_when_done.isChecked(),
            "shutdown": self.shutdown_when_done.isChecked(),
        }

    def _toggle_details(self) -> None:
        show = not self.details.isVisible()
        self.details.setVisible(show)
        self.details_btn.setText("<< Hide details" if show else "Show details >>")
        self.adjustSize()

    def _apply_limit(self) -> None:
        kbps = self.limit.value() if self.use_limit.isChecked() else 0
        task = self.manager.task(self.id)
        if task:
            task.set_speed_limit(kbps)
        if self.remember_limit.isChecked():
            self.d.speed_limit_kbps = kbps

    def _pause_resume(self) -> None:
        if self.manager.is_running(self.id):
            self.manager.stop(self.id)
        else:
            self.manager.start(self.id)
        self.refresh()

    def _cancel(self) -> None:
        self.manager.stop(self.id)
        self.close()

    def refresh(self) -> None:
        if self.id not in self.manager.downloads:
            self.close()
            return
        d = self.d
        task = self.manager.task(self.id)
        running = self.manager.is_running(self.id)
        snap = task.snapshot() if task else {"segments": d.segments, "connections": [], "rate": 0.0, "time_left": -1}
        pct = d.progress * 100
        self.setWindowTitle(f"{pct:.0f}% {d.filename}")
        self.progress.setValue(int(d.progress * 1000))
        detail = getattr(task, "status_text", "") if running else ""
        if detail:
            self.status.setText(detail)
        elif d.status == Status.ERROR:
            self.status.setText(f"Error: {d.error}")
        elif d.status == Status.PAUSED:
            self.status.setText("Paused")
        elif d.status in (Status.DOWNLOADING, Status.ASSEMBLING):
            self.status.setText("Receiving data..." if d.status == Status.DOWNLOADING else "Assembling...")
        elif d.status == Status.COMPLETE:
            self.status.setText("Complete")
        else:
            self.status.setText("Connecting...")
        self.file_size.setText(fmt.size(d.size) or "(unknown)")
        tail = f" ( {pct:.3f} % )" if d.size > 0 else ""
        self.downloaded.setText(fmt.size(d.downloaded) + tail)
        rate = fmt.rate(snap["rate"]) if running else ""
        self.rate.setText(rate)
        self.rate2.setText(rate)
        self.time_left.setText(fmt.duration(snap["time_left"]) if running and snap["time_left"] >= 0 else "(unknown)")
        self.resume.setText({True: "Yes", False: "No"}.get(d.resumable, "(unknown)"))
        self.pause_btn.setText("Pause" if running else "Resume")
        self.pause_btn.setEnabled(d.status != Status.COMPLETE)
        self.bar.set_segments(d.size, snap["segments"])
        conns = snap["connections"]
        while self.conns.topLevelItemCount() > len(conns):
            self.conns.takeTopLevelItem(self.conns.topLevelItemCount() - 1)
        for i, (n, received, info, alive, conn_rate) in enumerate(conns):
            item = self.conns.topLevelItem(i) or QTreeWidgetItem(self.conns)
            item.setText(0, str(n))
            item.setText(1, fmt.size(received))
            item.setText(2, fmt.rate(conn_rate) if alive and running else "")
            item.setText(3, info if alive or running else "")


class CompleteDialog(QDialog):
    """The "Download complete" dialog."""

    def __init__(self, d: Download, parent=None):
        super().__init__(parent)
        self.path = d.path
        self.setWindowTitle("Download complete")
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.resize(dlu(303, 132).width(), 0)
        lay = QVBoxLayout(self)
        top = QHBoxLayout()
        pic = QLabel()
        pic.setPixmap(file_icon(d.filename).pixmap(32, 32))
        top.addWidget(pic, 0, Qt.AlignTop)
        info = QLabel(f"<b>Download complete</b><br>Downloaded {fmt.size(d.size)} ({d.size:,} Bytes)")
        top.addWidget(info, 1)
        lay.addLayout(top)
        lay.addWidget(QLabel("Address"))
        addr = QLineEdit(d.url)
        addr.setReadOnly(True)
        lay.addWidget(addr)
        lay.addWidget(QLabel("The file saved as"))
        saved = QLineEdit(d.path)
        saved.setReadOnly(True)
        lay.addWidget(saved)
        row = QHBoxLayout()
        for text, slot in (("Open", self._open), ("Open with...", self._open_with), ("Open folder", self._folder)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            row.addWidget(b)
        row.addStretch(1)
        close = QPushButton("Close")
        close.setDefault(True)
        close.clicked.connect(self.accept)
        row.addWidget(close)
        lay.addLayout(row)
        self.dont_show = QCheckBox("Don't show this dialog again")
        lay.addWidget(self.dont_show)

    def _open(self):
        open_path(self.path)
        self.accept()

    def _open_with(self):
        cmd, ok = QInputDialog.getText(self, "Open with", "Command (the file path is appended):")
        if ok and cmd.strip():
            subprocess.Popen(shlex.split(cmd) + [self.path], start_new_session=True)
            self.accept()

    def _folder(self):
        open_folder(self.path)
        self.accept()


def choose_file(parent, start: str) -> str:
    path, _ = QFileDialog.getOpenFileName(parent, "Choose a file", start or os.path.expanduser("~"))
    return path
