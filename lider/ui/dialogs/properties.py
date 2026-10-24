"""The "File Properties" dialog."""

from __future__ import annotations

import os
import shutil

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QVBoxLayout,
)

from ...core import fmt
from ...core.manager import DownloadManager
from ...core.models import Status
from ..icons import file_icon
from ..util import dlu, open_path, separator
from ..widgets import primary


class PropertiesDialog(QDialog):
    def __init__(self, manager: DownloadManager, download_id: int, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.d = d = manager.downloads[download_id]
        self.setWindowTitle("File Properties")
        self.resize(dlu(318, 246).width(), 0)
        lay = QVBoxLayout(self)

        head = QHBoxLayout()
        pic = QLabel()
        pic.setPixmap(file_icon(d.filename).pixmap(32, 32))
        self.name = QLineEdit(d.filename)
        head.addWidget(pic)
        head.addWidget(self.name, 1)
        lay.addLayout(head)
        lay.addWidget(separator())

        g = QGridLayout()
        mime = os.path.splitext(d.filename)[1].lstrip(".").upper() or "File"
        rows = [("Type:", QLabel(f"{mime} file  ({d.category})")),
                ("Status:", QLabel(fmt.status(d) or "Queued" if d.status != Status.ERROR else f"Error: {d.error}")),
                ("Size:", QLabel(f"{fmt.size(d.size)} ({d.size:,} Bytes)" if d.size >= 0 else "(unknown)"))]
        if d.media:
            rows.append(("YouTube:", QLabel(d.media["variant"]["label"])))
        for r, (t, w) in enumerate(rows):
            g.addWidget(QLabel(t), r, 0)
            g.addWidget(w, r, 1, 1, 2)
        r = len(rows)
        self.save_dir = QLineEdit(d.save_dir)
        browse = QPushButton("&Browse...")
        browse.clicked.connect(self._browse)
        g.addWidget(QLabel("Save To:"), r, 0)
        g.addWidget(self.save_dir, r, 1)
        g.addWidget(browse, r, 2)
        self.address = QLineEdit(d.url)
        self.address.setReadOnly(bool(d.media))
        self.description = QLineEdit(d.description)
        g.addWidget(QLabel("Address:"), r + 1, 0)
        g.addWidget(self.address, r + 1, 1, 1, 2)
        g.addWidget(QLabel("Description:"), r + 2, 0)
        g.addWidget(self.description, r + 2, 1, 1, 2)
        g.setColumnStretch(1, 1)
        lay.addLayout(g)
        lay.addWidget(separator())
        lay.addWidget(QLabel("The web page from which this file was obtained:"))
        page = QLabel(d.parent_page or "Unknown")
        page.setTextInteractionFlags(Qt.TextSelectableByMouse)
        lay.addWidget(page)
        lay.addWidget(separator())

        g2 = QGridLayout()
        self.referer = QLineEdit(d.referer)
        self.login = QLineEdit(d.username)
        self.password = QLineEdit(d.password)
        self.password.setEchoMode(QLineEdit.Password)
        g2.addWidget(QLabel("Referer:"), 0, 0)
        g2.addWidget(self.referer, 0, 1, 1, 2)
        g2.addWidget(QLabel("Login"), 1, 0)
        g2.addWidget(self.login, 1, 1)
        g2.addWidget(QLabel("Password"), 2, 0)
        g2.addWidget(self.password, 2, 1)
        g2.setColumnStretch(2, 1)
        lay.addLayout(g2)
        lay.addWidget(separator())

        bottom = QHBoxLayout()
        info = QVBoxLayout()
        info.addWidget(QLabel(f"Last try: {fmt.date(d.last_try) or 'never'}"))
        info.addWidget(QLabel(f"Result: {'Error: ' + d.error if d.error else fmt.status(d)}"))
        bottom.addLayout(info, 1)
        open_btn = QPushButton("Open")
        open_btn.setEnabled(d.status == Status.COMPLETE)
        open_btn.clicked.connect(lambda: open_path(d.path))
        ok = primary(QPushButton("OK"))
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        bottom.addWidget(open_btn)
        bottom.addWidget(ok)
        lay.addLayout(bottom)

    def _browse(self):
        path = QFileDialog.getExistingDirectory(self, "Save To", self.save_dir.text())
        if path:
            self.save_dir.setText(path)

    def accept(self):
        d = self.d
        new_name = self.name.text().strip() or d.filename
        new_dir = self.save_dir.text().strip() or d.save_dir
        if (new_name, new_dir) != (d.filename, d.save_dir):
            if self.manager.is_running(d.id):
                QMessageBox.warning(self, "File Properties", "Stop the download before moving or renaming it.")
                return
            if d.status == Status.COMPLETE and os.path.exists(d.path):
                target = os.path.join(new_dir, new_name)
                try:
                    os.makedirs(new_dir, exist_ok=True)
                    shutil.move(d.path, target)
                except OSError as e:
                    QMessageBox.warning(self, "File Properties", f"Cannot move the file:\n{e}")
                    return
            d.filename, d.save_dir = new_name, new_dir
        d.url = self.address.text().strip() or d.url
        d.description = self.description.text()
        d.referer = self.referer.text().strip()
        d.username, d.password = self.login.text(), self.password.text()
        self.manager.update(d)
        super().accept()
