"""The "File Properties" dialog."""

from __future__ import annotations

import mimetypes
import os
import shutil

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QSizePolicy,
    QVBoxLayout,
)

from ...core import fmt
from ...core.manager import DownloadManager
from ...core.models import Status
from ..icons import category_icon, icon
from ..util import open_path
from ..widgets import caption, form_label, form_row, mono_font, panel, pill_label, primary, shell


class PropertiesDialog(QDialog):
    def __init__(self, manager: DownloadManager, download_id: int, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.d = d = manager.downloads[download_id]
        self.setWindowTitle("File Properties")
        self.setMinimumWidth(660)
        body, footer = shell(self, spacing=12)

        # -- file tile, editable name, status and category pills
        head = QHBoxLayout()
        head.setSpacing(14)
        tile = QFrame()
        tile.setObjectName("tile")
        tile.setFixedSize(52, 52)
        tl = QVBoxLayout(tile)
        tl.setContentsMargins(0, 0, 0, 0)
        pic = QLabel()
        pic.setAlignment(Qt.AlignCenter)
        pic.setPixmap(icon(category_icon(d.category)).pixmap(30, 30))
        tl.addWidget(pic)
        head.addWidget(tile, 0, Qt.AlignTop)
        name_col = QVBoxLayout()
        name_col.setSpacing(8)
        self.name = QLineEdit(d.filename)
        name_col.addWidget(self.name)
        pills = QHBoxLayout()
        pills.setSpacing(6)
        status = fmt.status(d) or "Queued"
        if d.status == Status.ERROR:
            pills.addWidget(pill_label(f"! Error: {d.error}" if d.error else "! Error", "bad"))
        elif d.status == Status.COMPLETE:
            pills.addWidget(pill_label("✓ Complete", "ok"))
        else:
            pills.addWidget(pill_label(status, "accent" if self.manager.is_running(d.id) else "muted"))
        pills.addWidget(pill_label(d.category, "muted"))
        pills.addStretch(1)
        name_col.addLayout(pills)
        head.addLayout(name_col, 1)
        body.addLayout(head)

        # -- file
        box, fl = panel()
        mime = mimetypes.guess_type(d.filename)[0] or (os.path.splitext(d.filename)[1].lstrip(".").upper() or "Unknown")
        fl.addLayout(form_row("Type", QLabel(f"{mime} file ({d.category})"), width=100))
        size = QLabel(f"{fmt.size(d.size)} ({d.size:,} Bytes)" if d.size >= 0 else "(unknown)")
        size.setFont(mono_font(size.font()))
        fl.addLayout(form_row("Size", size, width=100))
        if d.media:
            fl.addLayout(form_row("YouTube", QLabel(d.media["variant"]["label"]), width=100))
        self.save_dir = QLineEdit(d.save_dir)
        browse = QPushButton("&Browse...")
        browse.clicked.connect(self._browse)
        fl.addLayout(form_row("Save To", self.save_dir, browse, width=100, stretch_last=False))
        self.save_dir.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Fixed)
        self.address = QLineEdit(d.url)
        self.address.setReadOnly(bool(d.media))
        self.description = QLineEdit(d.description)
        fl.addLayout(form_row("Address", self.address, width=100))
        fl.addLayout(form_row("Description", self.description, width=100))
        for e in (self.save_dir, self.address):
            e.setFont(mono_font(e.font()))
            e.setCursorPosition(0)
        body.addWidget(box)

        # -- web page and authorization
        box, wl = panel()
        wl.addWidget(caption("Web page and authorization"))
        page = QLabel(d.parent_page or "Unknown")
        page.setObjectName("link" if d.parent_page else "muted")
        page.setFont(mono_font(page.font()))
        page.setTextInteractionFlags(Qt.TextSelectableByMouse)
        page.setMinimumWidth(1)
        wl.addLayout(form_row("Parent page", page, width=100))
        self.referer = QLineEdit(d.referer)
        self.referer.setFont(mono_font(self.referer.font()))
        wl.addLayout(form_row("Referer", self.referer, width=100))
        self.login = QLineEdit(d.username)
        self.password = QLineEdit(d.password)
        self.password.setEchoMode(QLineEdit.Password)
        auth = form_row("Login", self.login, width=100)
        auth.addSpacing(10)
        auth.addWidget(form_label("Password"))
        auth.addWidget(self.password, 1)
        wl.addLayout(auth)
        body.addWidget(box)

        result = f"Error: {d.error}" if d.error else fmt.status(d)
        last = form_label(f"Last try: {fmt.date(d.last_try) or 'never'} · Result: {result}")
        body.addWidget(last)

        open_btn = QPushButton("Open")
        open_btn.setEnabled(d.status == Status.COMPLETE)
        open_btn.clicked.connect(lambda: open_path(d.path))
        ok = primary(QPushButton("OK"))
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        footer.addStretch(1)
        footer.addWidget(open_btn)
        footer.addWidget(ok)
        ok.setFocus()

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
