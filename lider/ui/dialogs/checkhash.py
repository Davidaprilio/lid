""""Check hash…" from the download list: check a file against a checksum, or just compute its hash."""

from __future__ import annotations

import os

from PySide6.QtCore import QSize, Qt, QTimer
from PySide6.QtWidgets import (
    QApplication,
    QCheckBox,
    QDialog,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QProgressBar,
    QPushButton,
    QVBoxLayout,
)

from ...core import checksum as checksums
from ...core import fmt
from ...core.checksum import Verifier
from ...core.manager import DownloadManager
from ...core.models import Download, Status
from ..icons import bind, icon
from ..widgets import ChecksumEdit, ToolButton, caption, mono_font, primary, shell

READY_HINT = ("Pre-filled with the checksum stored for this download. Paste the value published by the site, "
              "or leave it empty to only compute the file's hash.")
RUNNING_HINT = ("Paste the value published by the site. Computing a hash without a value is available once the "
                "file is complete.")


def can_check(manager: DownloadManager, d: Download) -> bool:
    """The menu item is enabled once a download has data on disk (or is running)."""
    return (d.status in (Status.COMPLETE, Status.VERIFYING) or d.downloaded > 0
            or manager.is_running(d.id))


def _mono_field(value: str = "") -> QLineEdit:
    e = QLineEdit(value)
    e.setReadOnly(True)
    e.setFont(mono_font(e.font()))
    e.setCursorPosition(0)
    e.setFocusPolicy(Qt.ClickFocus)
    return e


class CheckHashDialog(QDialog):
    """Ready → Reading → Match / Mismatch / Computed for a finished file; "Check when finished" for one
    that is still downloading. ``choice`` is "delete", "again" or "keep" after a mismatch."""

    def __init__(self, manager: DownloadManager, download_id: int, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.d = d = manager.downloads[download_id]
        self.choice = ""
        self.verifier: Verifier | None = None
        self.finished_file = d.status in (Status.COMPLETE, Status.VERIFYING)
        self.setWindowTitle("Check hash")
        self.setMinimumWidth(640)
        if self.finished_file or d.size <= 0:
            sub = f"{d.filename} · {fmt.size(d.size if d.size > 0 else d.downloaded)}"
        else:
            sub = f"{d.filename} · {fmt.size(d.downloaded).split()[0]} of {fmt.size(d.size)}"
        body, footer = shell(self, "checksum", "Check hash", sub, spacing=8, badge="accent")

        body.addWidget(caption("Expected checksum"))
        self.check = ChecksumEdit(d.checksum, hint=READY_HINT if self.finished_file else RUNNING_HINT)
        body.addWidget(self.check)
        body.addWidget(self.check.message)
        self.save = QCheckBox("Save this checksum to the download")
        self.save.setChecked(True)
        body.addWidget(self.save)

        # -- still downloading: an accent note instead of the save box
        self.note = QFrame()
        self.note.setObjectName("nextRun")
        nl = QHBoxLayout(self.note)
        nl.setContentsMargins(12, 9, 12, 9)
        nl.setSpacing(8)
        clock = QLabel()
        clock.setPixmap(icon("clock").pixmap(14, 14))
        pct = f" ({d.progress * 100:.0f}%)" if d.size > 0 else ""
        text = QLabel(f"The file is still downloading{pct}. LID checks it as soon as the download finishes.")
        text.setWordWrap(True)
        nl.addWidget(clock, 0, Qt.AlignTop)
        nl.addWidget(text, 1)
        body.addWidget(self.note)

        # -- reading
        self.reading = QFrame()
        rl = QVBoxLayout(self.reading)
        rl.setContentsMargins(0, 6, 0, 0)
        rl.setSpacing(6)
        top = QHBoxLayout()
        self.read_label = QLabel()
        f = self.read_label.font()
        f.setBold(True)
        self.read_label.setFont(f)
        self.read_size = QLabel()
        self.read_size.setObjectName("muted")
        self.read_size.setFont(mono_font(self.read_size.font()))
        top.addWidget(self.read_label)
        top.addStretch(1)
        top.addWidget(self.read_size)
        rl.addLayout(top)
        bar_row = QHBoxLayout()
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.bar.setTextVisible(False)
        self.bar.setFixedHeight(10)
        stop = ToolButton(pad=14, gap=6)
        stop.setObjectName("compact")
        stop.setText("Stop")
        stop.setToolButtonStyle(Qt.ToolButtonTextBesideIcon)
        stop.setFixedHeight(30)
        stop.clicked.connect(self._stop)
        bar_row.addWidget(self.bar, 1)
        bar_row.addWidget(stop)
        rl.addLayout(bar_row)
        self.read_note = QLabel()
        self.read_note.setObjectName("faint")
        rl.addWidget(self.read_note)
        body.addWidget(self.reading)

        # -- results
        self.match = QFrame()
        self.match.setObjectName("matchBox")
        ml = QVBoxLayout(self.match)
        ml.setContentsMargins(14, 12, 14, 12)
        ml.setSpacing(8)
        head = QHBoxLayout()
        shield = QLabel()
        shield.setPixmap(icon("shield-check").pixmap(16, 16))
        title = QLabel("Matches — the file is intact")
        title.setObjectName("okText")
        f = title.font()
        f.setBold(True)
        title.setFont(f)
        self.match_time = QLabel()
        self.match_time.setObjectName("okText")
        self.match_time.setFont(mono_font(self.match_time.font()))
        head.addWidget(shield)
        head.addWidget(title)
        head.addStretch(1)
        head.addWidget(self.match_time)
        ml.addLayout(head)
        self.match_caption = caption("Computed SHA-256")
        ml.addWidget(self.match_caption)
        self.match_value = _mono_field()
        ml.addLayout(self._with_copy(self.match_value))
        body.addWidget(self.match)

        self.mismatch = QFrame()
        self.mismatch.setObjectName("mismatchBox")
        gl = QGridLayout(self.mismatch)
        gl.setContentsMargins(14, 12, 14, 12)
        gl.setHorizontalSpacing(16)
        gl.setVerticalSpacing(6)
        self.expected_value, self.actual_value = QLabel(), QLabel()
        for row, (name, lab) in enumerate((("Expected", self.expected_value), ("Actual", self.actual_value))):
            lab.setFont(mono_font(lab.font()))
            lab.setTextInteractionFlags(Qt.TextSelectableByMouse)
            lab.setMinimumWidth(1)
            gl.addWidget(caption(name), row, 0)
            gl.addWidget(lab, row, 1)
        self.actual_value.setObjectName("badText")
        self.mismatch_note = QLabel()
        self.mismatch_note.setObjectName("muted")
        self.mismatch_note.setWordWrap(True)
        gl.addWidget(self.mismatch_note, 2, 0, 1, 2)
        gl.setColumnStretch(1, 1)
        body.addWidget(self.mismatch)

        self.computed = QFrame()
        self.computed.setObjectName("panel")
        cl = QVBoxLayout(self.computed)
        cl.setContentsMargins(14, 12, 14, 12)
        cl.setSpacing(8)
        self.computed_caption = caption("SHA-256 of the file")
        cl.addWidget(self.computed_caption)
        self.computed_value = _mono_field()
        cl.addLayout(self._with_copy(self.computed_value))
        body.addWidget(self.computed)

        self.result_note = QLabel()
        self.result_note.setObjectName("faint")
        self.result_note.setWordWrap(True)
        body.addWidget(self.result_note)
        self.missing = QLabel(f"File not found at {d.path}")
        self.missing.setObjectName("errorText")
        self.missing.setWordWrap(True)
        body.addWidget(self.missing)

        # -- footer: every button, shown per state
        self.delete_btn = QPushButton("Delete file")
        self.delete_btn.setObjectName("dangerBtn")
        bind(self.delete_btn, "delete")
        self.delete_btn.setIconSize(QSize(14, 14))
        self.again_btn = primary(QPushButton("Download again"))
        self.again_btn.setIcon(icon("add-white"))
        self.again_btn.setIconSize(QSize(14, 14))
        self.keep_btn = QPushButton("Keep file")
        self.retry_btn = QPushButton("Check again")
        self.close_btn = QPushButton("Close")
        self.close_primary = primary(QPushButton("Close"))
        self.run_btn = primary(QPushButton("Check now"))
        self.later_btn = primary(QPushButton("Check when finished"))
        self.delete_btn.clicked.connect(lambda: self._done("delete"))
        self.again_btn.clicked.connect(lambda: self._done("again"))
        self.keep_btn.clicked.connect(lambda: self._done("keep"))
        self.retry_btn.clicked.connect(lambda: self._show("ready"))
        self.close_btn.clicked.connect(self.reject)
        self.close_primary.clicked.connect(self.reject)
        self.run_btn.clicked.connect(self._run)
        self.later_btn.clicked.connect(self._later)
        footer.addWidget(self.delete_btn)
        footer.addStretch(1)
        for b in (self.retry_btn, self.close_btn, self.again_btn, self.keep_btn, self.close_primary,
                  self.run_btn, self.later_btn):
            footer.addWidget(b)

        self.timer = QTimer(self)
        self.timer.timeout.connect(self._poll)
        self.check.changed.connect(self._field_changed)
        self._show("ready" if self.finished_file else "running")

    @staticmethod
    def _with_copy(field: QLineEdit) -> QHBoxLayout:
        row = QHBoxLayout()
        copy = QPushButton("Copy")
        copy.clicked.connect(lambda: QApplication.clipboard().setText(field.text()))
        row.addWidget(field, 1)
        row.addWidget(copy)
        return row

    # -- states
    def _show(self, state: str) -> None:
        self.state = state
        visible = {
            "ready": {self.save, self.close_btn, self.run_btn},
            "reading": {self.save, self.reading, self.close_btn, self.run_btn},
            "match": {self.save, self.match, self.result_note, self.retry_btn, self.close_primary},
            "mismatch": {self.save, self.mismatch, self.delete_btn, self.again_btn, self.keep_btn},
            "computed": {self.save, self.computed, self.result_note, self.retry_btn, self.close_primary},
            "running": {self.note, self.close_btn, self.later_btn},
        }[state]
        for w in (self.save, self.note, self.reading, self.match, self.mismatch, self.computed, self.result_note,
                  self.delete_btn, self.again_btn, self.keep_btn, self.retry_btn, self.close_btn,
                  self.close_primary, self.run_btn, self.later_btn):
            w.setVisible(w in visible)
        self.check.setEnabled(state in ("ready", "running", "match", "computed", "mismatch"))
        missing = self.finished_file and not os.path.isfile(self.d.path)
        self.missing.setVisible(missing and state == "ready")
        self._field_changed()
        default = {"ready": self.run_btn, "running": self.later_btn, "match": self.close_primary,
                   "computed": self.close_primary, "mismatch": self.again_btn}.get(state)
        if default is not None:
            default.setDefault(True)
            default.setFocus()  # Enter runs the main action; paste into the field with a click
        self.adjustSize()

    def _field_changed(self) -> None:
        valid = self.check.isValid()
        empty = not self.check.edit.text().strip()
        self.save.setEnabled(valid and not empty and self.state in ("ready", "reading"))
        self.run_btn.setText("Compute hash" if empty else "Check now")
        self.run_btn.setEnabled(valid and self.state == "ready" and os.path.isfile(self.d.path))
        self.later_btn.setEnabled(valid and not empty)
        if self.state in ("match", "computed", "mismatch") and self.sender() is self.check:
            self._show("ready")  # a new value means a new check

    # -- actions
    def _algorithm(self) -> tuple[str, str]:
        """(checksum to compare with or "", algorithm to compute)."""
        value = self.check.value()
        if value:
            return value, checksums.split(value)[0]
        algo = self.check.algorithm.currentData()
        return "", algo if algo != "auto" else "sha256"  # nothing to auto-detect: SHA-256

    def _run(self) -> None:
        value, algo = self._algorithm()
        self.verifier = Verifier(self.d.path, value or f"{algo}:", download_id=self.d.id)
        self.verifier.start()
        self._title = checksums.TITLES[algo]
        self._show("reading")
        self._poll()
        self.timer.start(100)

    def _poll(self) -> None:
        v = self.verifier
        if v is None:
            return
        self.read_label.setText(f"Reading the file… {v.progress * 100:.0f}%")
        self.read_size.setText(f"{fmt.size(v.read)} of {fmt.size(v.total)}" if v.total else "")
        self.bar.setValue(int(v.progress * 1000))
        self.read_note.setText(f"{self._title} · runs in the background; the list stays usable.")
        if v.running:
            return
        self.timer.stop()
        self.verifier = None
        self._result(v)

    def _result(self, v: Verifier) -> None:
        value, algo = self._algorithm()
        title = checksums.TITLES[algo]
        saving = self.save.isChecked() and bool(value) and v.result in ("verified", "mismatch")
        if saving:
            self.manager.record_check(self.d.id, value, v.result, v.actual, v.seconds)
        if v.result == "verified":
            self.match_time.setText(f"{title} · {v.seconds:.1f} s")
            self.match_caption.setText(f"Computed {title}".upper())
            self.match_value.setText(v.actual)
            self.match_value.setCursorPosition(0)
            self.result_note.setText("Saved to the download — the list shows ✓ Verified." if saving
                                     else "Not saved — the download is unchanged.")
            self._show("match")
        elif v.result == "mismatch":
            self.expected_value.setText(checksums.split(value)[1])
            self.actual_value.setText(v.actual)
            note = f"{title}. Expected is the value above; Actual is computed from the file on disk."
            if saving:
                note += " Saved to the download — the list shows Checksum mismatch."
            self.mismatch_note.setText(note)
            self._show("mismatch")
        elif v.result == "computed":
            self.computed_caption.setText(f"{title} of the file".upper())
            self.computed_value.setText(v.actual)
            self.computed_value.setCursorPosition(0)
            self.result_note.setText("No expected value, so there is no verdict and nothing is saved. Pick MD5, "
                                     "SHA-1 or SHA-512 above to compute another type.")
            self._show("computed")
        else:  # stopped, or the file could not be read
            self._show("ready")
            if v.result == "error":
                self.missing.setText(f"Could not read the file: {v.error}")
                self.missing.setVisible(True)

    def _stop(self) -> None:
        if self.verifier:
            self.verifier.skip()  # nothing is saved

    def _later(self) -> None:
        self.manager.check_when_finished(self.d.id, self.check.value())
        self.accept()

    def _done(self, choice: str) -> None:
        self.choice = choice
        self.accept()

    def done(self, r: int) -> None:
        if self.verifier:  # Close while reading stops first
            self.verifier.skip()
            self.verifier.join(2)
            self.verifier = None
        self.timer.stop()
        super().done(r)
