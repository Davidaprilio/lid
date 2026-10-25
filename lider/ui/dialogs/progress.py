"""Download progress (status, speed limiter, options on completion) and Download complete."""

from __future__ import annotations

import os
import shlex
import subprocess

from PySide6.QtCore import QRectF, QSize, Qt, QTimer
from PySide6.QtGui import QColor, QFont, QIcon, QPainter, QPainterPath
from PySide6.QtWidgets import (
    QCheckBox,
    QDialog,
    QFileDialog,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QProgressBar,
    QPushButton,
    QSpinBox,
    QStackedWidget,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ...core import checksum as checksums
from ...core import fmt
from ...core.manager import DownloadManager
from ...core.models import Download, Segment, Status
from .. import theme
from ..icons import bind, category_icon, icon
from ..util import open_folder, open_path
from ..widgets import (InfoPillDelegate, caption, check_pixmap, form_label, form_row, mono_font, panel, pill_label,
                       primary, set_pill, shell, small_header, tab_strip)


class SegmentBar(QWidget):
    """Draws each connection's segment: downloaded part filled, the rest empty."""

    def __init__(self, parent=None, gap: str = "pane"):
        super().__init__(parent)
        self.size_total = 0
        self.segments: list[Segment] = []
        self.gap = gap  # the colour behind the bar, drawn between segments
        self.setFixedHeight(12)

    def set_segments(self, size: int, segments: list[Segment]) -> None:
        self.size_total, self.segments = size, segments
        self.update()

    def paintEvent(self, _event):
        c = theme.colors()
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        r = QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5)
        clip = QPainterPath()
        clip.addRoundedRect(r, 5, 5)
        p.fillPath(clip, QColor(c["track"]))
        p.setClipPath(clip)
        if self.size_total > 0:
            scale = r.width() / self.size_total
            for s in self.segments:
                end = s.pos if s.end < 0 else min(s.pos, s.end)
                done = s.end >= 0 and s.pos >= s.end
                p.fillRect(QRectF(r.left() + s.start * scale, r.top(), max(1.0, (end - s.start) * scale), r.height()),
                           QColor(c["ok"] if done else c["ac"]))
            gap = QColor(c[self.gap])
            for s in self.segments[1:]:
                p.fillRect(QRectF(r.left() + s.start * scale - 1, r.top(), 2, r.height()), gap)


def _mono(lab: QLabel, scale: float = 1.0, bold: bool = False) -> QLabel:
    f = mono_font(lab.font())
    f.setPointSizeF(lab.font().pointSizeF() * scale)
    f.setBold(bold)
    lab.setFont(f)
    return lab


class ProgressDialog(QDialog):
    def __init__(self, manager: DownloadManager, download_id: int, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.id = download_id
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.setWindowFlag(Qt.WindowMinimizeButtonHint, True)
        self.setMinimumWidth(600)
        d = self.d

        # -- Download status tab
        status_tab, sl = panel()
        variant = (d.media or {}).get("variant")
        self.url_label = _mono(QLabel(f"{d.url}   ({variant['quality']} {variant['ext'].upper()})" if variant else d.url))
        self.url_label.setObjectName("link")
        self.url_label.setTextInteractionFlags(Qt.TextSelectableByMouse)
        self.url_label.setMinimumWidth(1)
        sl.addWidget(self.url_label)
        self.status = pill_label("Connecting…", "accent")
        self.file_size = _mono(QLabel("(unknown)"))
        self.downloaded = _mono(QLabel("0 Bytes"))
        self.rate = _mono(QLabel(""))
        self.time_left = _mono(QLabel("(unknown)"))
        self.resume = pill_label("(unknown)", "muted")
        g = QGridLayout()
        g.setHorizontalSpacing(0)
        g.setVerticalSpacing(9)
        for row, (title, lab) in enumerate((("Status", self.status), ("File size", self.file_size),
                                            ("Downloaded", self.downloaded), ("Transfer rate", self.rate),
                                            ("Time left", self.time_left), ("Resume capability", self.resume))):
            g.addWidget(form_label(title, 150), row, 0)
            g.addWidget(lab, row, 1, Qt.AlignLeft)
        g.setColumnStretch(1, 1)
        sl.addLayout(g)

        # -- Speed Limiter tab
        limit_tab, ll = panel()
        self.rate2 = _mono(QLabel(""))
        self.use_limit = QCheckBox("Use Speed Limiter")
        self.use_limit.setChecked(d.speed_limit_kbps > 0)
        self.limit = QSpinBox()
        self.limit.setRange(1, 10_000_000)
        self.limit.setValue(d.speed_limit_kbps or self.manager.settings.speed_limit_kbps)
        self.limit.setFont(mono_font(self.limit.font()))
        self.limit.setFixedWidth(110)
        self.remember_limit = QCheckBox("Remember Speed Limiter settings for this file on download start")
        self.remember_limit.setChecked(True)
        ll.addLayout(form_row("Transfer rate", self.rate2, width=130))
        ll.addWidget(self.use_limit)
        unit = form_label("KBytes/sec")
        ll.addLayout(form_row("Maximum speed", self.limit, unit, QWidget(), width=130))
        ll.addWidget(self.remember_limit)
        ll.addStretch(1)
        self.use_limit.toggled.connect(self._apply_limit)
        self.limit.valueChanged.connect(self._apply_limit)

        # -- Options on completion tab
        done_tab, dg = panel()
        self.save_to = QLineEdit(d.path)
        self.save_to.setReadOnly(True)
        self.save_to.setFont(mono_font(self.save_to.font()))
        self.save_to.setCursorPosition(0)
        self.show_complete = QCheckBox("Show download complete dialog")
        self.show_complete.setChecked(self.manager.settings.show_complete_dialog)
        self.exit_when_done = QCheckBox("Exit LID when done")
        self.shutdown_when_done = QCheckBox("Turn off computer when done")
        dg.addWidget(self.show_complete)
        dg.addLayout(form_row("Save To", self.save_to, width=72))
        dg.addWidget(caption("Power options"))
        dg.addWidget(self.exit_when_done)
        dg.addWidget(self.shutdown_when_done)
        dg.addStretch(1)

        strip, self.tabs = tab_strip(["Download status", "Speed Limiter", "Options on completion"])
        self.pages = QStackedWidget()
        for page in (status_tab, limit_tab, done_tab):
            self.pages.addWidget(page)
        self.tabs.currentChanged.connect(self.pages.setCurrentIndex)

        self.percent = _mono(QLabel("0.0%"), 20 / 12, True)
        self.progress = QProgressBar()
        self.progress.setRange(0, 1000)
        self.progress.setTextVisible(False)
        self.progress.setFixedHeight(10)
        prow = QHBoxLayout()
        prow.setSpacing(10)
        prow.addWidget(self.percent)
        prow.addWidget(self.progress, 1)

        self.details_btn = QPushButton("<< Hide details")
        self.details_btn.setObjectName("ghostBtn")
        self.pause_btn = QPushButton("Pause")
        self.pause_btn.setIconSize(QSize(14, 14))
        cancel = QPushButton("Cancel")
        for b in (self.pause_btn, cancel):
            b.setMinimumWidth(96)
        self.details_btn.clicked.connect(self._toggle_details)
        self.pause_btn.clicked.connect(self._pause_resume)
        cancel.clicked.connect(self._cancel)
        buttons = QHBoxLayout()
        buttons.setSpacing(8)
        buttons.addWidget(self.details_btn)
        buttons.addStretch(1)
        buttons.addWidget(self.pause_btn)
        buttons.addWidget(cancel)

        # -- details: segment bar + connection list
        self.details = QWidget()
        dl = QVBoxLayout(self.details)
        dl.setContentsMargins(0, 0, 0, 0)
        dl.setSpacing(12)
        title = QLabel("Start positions and download progress by connections")
        title.setObjectName("muted")
        f = title.font()
        f.setWeight(QFont.DemiBold)
        title.setFont(f)
        dl.addWidget(title)
        self.bar = SegmentBar(gap="win")
        dl.addWidget(self.bar)
        self.conns = QTreeWidget()
        self.conns.setHeaderLabels(["N.", "DOWNLOADED", "SPEED", "INFO"])
        self.conns.setRootIsDecorated(False)
        self.conns.setAlternatingRowColors(True)
        self.conns.setItemDelegateForColumn(3, InfoPillDelegate(self.conns))
        self.conns.setColumnWidth(0, 50)
        self.conns.setColumnWidth(1, 110)
        self.conns.setColumnWidth(2, 100)
        self.conns.setMinimumHeight(136)
        small_header(self.conns)
        dl.addWidget(self.conns)

        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addWidget(strip)
        body = QVBoxLayout()
        body.setContentsMargins(18, 14, 18, 14)
        body.setSpacing(12)
        body.addWidget(self.pages)
        body.addLayout(prow)
        body.addLayout(buttons)
        body.addWidget(self.details, 1)
        lay.addLayout(body, 1)

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
        if self.d.status == Status.VERIFYING:
            self.manager.skip_check(self.id)
        elif self.manager.is_running(self.id):
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
        self.percent.setText(f"{pct:.1f}%" if d.size > 0 else fmt.size(d.downloaded))
        detail = getattr(task, "status_text", "") if running else ""
        verifying = self.manager.verify_progress(self.id)
        if verifying is not None:
            set_pill(self.status, f"Verifying {verifying * 100:.0f}%", "accent")
        elif detail:
            set_pill(self.status, detail, "accent")
        elif d.status == Status.ERROR:
            set_pill(self.status, f"Error: {d.error}", "bad")
        elif d.status == Status.PAUSED:
            set_pill(self.status, "Paused", "muted")
        elif d.status in (Status.DOWNLOADING, Status.ASSEMBLING):
            set_pill(self.status, "⤓ Receiving data…" if d.status == Status.DOWNLOADING else "Assembling…", "accent")
        elif d.status == Status.COMPLETE:
            set_pill(self.status, "✓ Complete", "ok")
        else:
            set_pill(self.status, "Connecting…", "accent")
        self.file_size.setText(fmt.size(d.size) or "(unknown)")
        tail = f" ({pct:.1f} %)" if d.size > 0 else ""
        self.downloaded.setText(fmt.size(d.downloaded) + tail)
        rate = fmt.rate(snap["rate"]) if running else ""
        self.rate.setText(rate)
        self.rate2.setText(rate)
        self.time_left.setText(fmt.duration(snap["time_left"]) if running and snap["time_left"] >= 0 else "(unknown)")
        if d.resumable is None:
            set_pill(self.resume, "(unknown)", "muted")
        else:
            set_pill(self.resume, "✓ Yes" if d.resumable else "No", "ok" if d.resumable else "muted")
        label = "Skip check" if verifying is not None else ("Pause" if running else "Resume")
        if self.pause_btn.text() != label:
            self.pause_btn.setText(label)
            if verifying is not None:
                self.pause_btn.setIcon(QIcon())
            else:
                bind(self.pause_btn, "stop" if running else "resume")
        self.pause_btn.setEnabled(d.status != Status.COMPLETE)
        self.bar.set_segments(d.size, snap["segments"])
        conns = snap["connections"]
        while self.conns.topLevelItemCount() > len(conns):
            self.conns.takeTopLevelItem(self.conns.topLevelItemCount() - 1)
        mono = mono_font(self.conns.font())
        for i, (n, received, info, alive, conn_rate) in enumerate(conns):
            item = self.conns.topLevelItem(i)
            if item is None:
                item = QTreeWidgetItem(self.conns)
                for col in range(3):
                    item.setFont(col, mono)
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
        self.setMinimumWidth(640)
        body, footer = shell(self, spacing=8)
        head = QHBoxLayout()
        head.setContentsMargins(0, 0, 0, 4)
        head.setSpacing(12)
        badge = QFrame()
        badge.setObjectName("okBadge")
        badge.setFixedSize(42, 42)
        bl = QVBoxLayout(badge)
        bl.setContentsMargins(0, 0, 0, 0)
        mark = QLabel()
        mark.setAlignment(Qt.AlignCenter)
        if d.checksum_result == "verified":
            mark.setPixmap(icon("shield-check").pixmap(22, 22))
        else:
            mark.setPixmap(check_pixmap(22, QColor(theme.colors()["ok"]), self.devicePixelRatioF()))
        bl.addWidget(mark)
        head.addWidget(badge, 0, Qt.AlignTop)
        text = QVBoxLayout()
        text.setSpacing(2)
        title = QLabel("Download complete")
        title.setObjectName("dialogTitle")
        f = title.font()
        f.setPointSizeF(f.pointSizeF() * 1.25)
        f.setBold(True)
        title.setFont(f)
        sub = QLabel(f"Downloaded {fmt.size(d.size)} ({d.size:,} Bytes)")
        sub.setObjectName("muted")
        text.addWidget(title)
        text.addWidget(sub)
        head.addLayout(text, 1)
        verified = d.checksum_result == "verified"
        if verified:
            head.addWidget(pill_label(f"{checksums.title(d.checksum)} verified", "ok"), 0, Qt.AlignTop)
        body.addLayout(head)

        addr = QLineEdit(d.url)
        saved = QLineEdit(d.path)
        for e in (addr, saved):
            e.setReadOnly(True)
            e.setFont(mono_font(e.font()))
            e.setCursorPosition(0)
            e.setFocusPolicy(Qt.ClickFocus)
        bind(saved.addAction(icon(category_icon(d.category)), QLineEdit.LeadingPosition), category_icon(d.category))
        body.addWidget(caption("Address"))
        body.addWidget(addr)
        body.addSpacing(4)
        body.addWidget(caption("The file saved as"))
        body.addWidget(saved)
        if verified:
            value = QLineEdit(checksums.split(d.checksum)[1])
            value.setReadOnly(True)
            value.setFont(mono_font(value.font()))
            value.setCursorPosition(0)
            value.setFocusPolicy(Qt.ClickFocus)
            value.addAction(QIcon(check_pixmap(14, QColor(theme.colors()["ok"]), self.devicePixelRatioF())),
                            QLineEdit.TrailingPosition)
            body.addSpacing(4)
            body.addWidget(caption(f"Checksum ({checksums.title(d.checksum)}) · matches"))
            body.addWidget(value)

        self.dont_show = QCheckBox("Don't show this dialog again")
        footer.addWidget(self.dont_show)
        footer.addStretch(1)
        for text, slot in (("Open", self._open), ("Open with...", self._open_with), ("Open folder", self._folder)):
            b = primary(QPushButton(text)) if slot == self._open else QPushButton(text)
            b.clicked.connect(slot)
            footer.addWidget(b)
        close = QPushButton("Close")
        close.setDefault(True)
        close.clicked.connect(self.accept)
        footer.addWidget(close)

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


class MismatchDialog(QDialog):
    """"The file doesn't match its checksum": expected and actual hash, then Delete file,
    Download again or Keep file. ``choice`` is "delete", "again" or "keep"."""

    def __init__(self, d: Download, parent=None):
        super().__init__(parent)
        self.choice = "keep"
        self.setWindowTitle("Checksum mismatch")
        self.setMinimumWidth(640)
        body, footer = shell(self, "shield-alert", "The file doesn't match its checksum",
                             f"{d.filename} · {fmt.size(d.size)}. It may be corrupt or changed on the server.",
                             spacing=8, badge="bad")
        box = QFrame()
        box.setObjectName("mismatchBox")
        bl = QGridLayout(box)
        bl.setContentsMargins(14, 12, 14, 12)
        bl.setHorizontalSpacing(16)
        bl.setVerticalSpacing(6)
        for row, (name, value, bad) in enumerate((("Expected", checksums.split(d.checksum)[1], False),
                                                   ("Actual", d.checksum_actual, True))):
            cap = caption(name)
            val = _mono(QLabel(value))
            val.setTextInteractionFlags(Qt.TextSelectableByMouse)
            val.setMinimumWidth(1)
            if bad:
                val.setObjectName("badText")
            bl.addWidget(cap, row, 0)
            bl.addWidget(val, row, 1)
        why = QLabel(f"{checksums.title(d.checksum)}. Expected is the value you entered; Actual is computed from "
                     "the downloaded file after its parts were merged.")
        why.setObjectName("muted")
        why.setWordWrap(True)
        bl.addWidget(why, 2, 0, 1, 2)
        bl.setColumnStretch(1, 1)
        body.addWidget(box)
        body.addSpacing(4)
        body.addWidget(caption("The file saved as"))
        saved = QLineEdit(d.path)
        saved.setReadOnly(True)
        saved.setFont(mono_font(saved.font()))
        saved.setCursorPosition(0)
        saved.setFocusPolicy(Qt.ClickFocus)
        bind(saved.addAction(icon(category_icon(d.category)), QLineEdit.LeadingPosition), category_icon(d.category))
        body.addWidget(saved)

        delete = QPushButton("Delete file")
        delete.setObjectName("dangerBtn")
        bind(delete, "delete")
        again = primary(QPushButton("Download again"))
        again.setIcon(icon("add-white"))
        keep = QPushButton("Keep file")
        for b in (delete, again):
            b.setIconSize(QSize(14, 14))
        again.setDefault(True)
        delete.clicked.connect(lambda: self._done("delete"))
        again.clicked.connect(lambda: self._done("again"))
        keep.clicked.connect(lambda: self._done("keep"))
        footer.addWidget(delete)  # apart from the safe actions
        footer.addStretch(1)
        footer.addWidget(again)
        footer.addWidget(keep)

    def _done(self, choice: str) -> None:
        self.choice = choice
        self.accept()


def choose_file(parent, start: str) -> str:
    path, _ = QFileDialog.getOpenFileName(parent, "Choose a file", start or os.path.expanduser("~"))
    return path
