"""Closing LID while downloads are still running."""

from __future__ import annotations

from PySide6.QtWidgets import QDialog, QHBoxLayout, QLabel, QPushButton

from ...core import fmt
from ...core.models import Download
from ..icons import category_icon, icon
from ..widgets import panel, primary, shell


class CloseWhileDownloadingDialog(QDialog):
    """Stop and close · Continue in background · Keep downloading. ``choice`` is "stop", "background"
    or "keep" (also when the dialog is dismissed)."""

    SHOWN = 3  # running downloads listed by name

    def __init__(self, running: list[Download], background: bool = True, exiting: bool = False, parent=None):
        super().__init__(parent)
        self.choice = "keep"
        self.setWindowTitle("LID")
        self.setMinimumWidth(560)
        n = len(running)
        title = f"{n} download{'s are' if n != 1 else ' is'} still running"
        what = "Exiting" if exiting else "Closing the window"
        sub = (f"{what} stops {'them' if n != 1 else 'it'}. Unfinished downloads can be resumed later."
               if not background else
               f"{what} stops {'them' if n != 1 else 'it'}. LID can keep downloading in the background, "
               "with an icon in the system tray.")
        body, footer = shell(self, "queue", title, sub, spacing=10, badge="accent")

        box, lay = panel(8)
        for d in running[: self.SHOWN]:
            row = QHBoxLayout()
            row.setSpacing(8)
            pic = QLabel()
            pic.setPixmap(icon(category_icon(d.category)).pixmap(16, 16))
            name = QLabel(d.filename)
            name.setMinimumWidth(1)
            done = QLabel(f"{d.progress * 100:.0f}%" if d.size > 0 else fmt.size(d.downloaded))
            done.setObjectName("muted")
            row.addWidget(pic)
            row.addWidget(name, 1)
            row.addWidget(done)
            lay.addLayout(row)
        if n > self.SHOWN:
            more = QLabel(f"and {n - self.SHOWN} more")
            more.setObjectName("faint")
            lay.addWidget(more)
        body.addWidget(box)

        stop = QPushButton("Stop and close" if not exiting else "Stop and exit")
        stop.setObjectName("dangerBtn")
        keep = QPushButton("Keep downloading")
        stop.clicked.connect(lambda: self._done("stop"))
        keep.clicked.connect(lambda: self._done("keep"))
        footer.addWidget(stop)  # apart from the safe choices
        footer.addStretch(1)
        if background:
            bg = primary(QPushButton("Continue in background"))
            bg.clicked.connect(lambda: self._done("background"))
            footer.addWidget(bg)
        footer.addWidget(keep)
        keep.setDefault(True)  # Enter and Esc both keep the window open
        keep.setFocus()

    def _done(self, choice: str) -> None:
        self.choice = choice
        self.accept()
