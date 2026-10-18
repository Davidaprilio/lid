"""Options dialog (General, File types, Save to, Downloads, Connection, Proxy) and its helper dialogs."""

from __future__ import annotations

import os
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QAbstractItemView,
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QGridLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPlainTextEdit,
    QPushButton,
    QSpinBox,
    QTabWidget,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ...core.settings import CONNECTION_CHOICES, DEFAULT_COLUMNS, DEFAULT_FILE_TYPES, Category, Settings
from ..util import dlu, page_header, separator

AUTOSTART = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "autostart" / "lider.desktop"


def set_autostart(on: bool) -> None:
    if on:
        AUTOSTART.parent.mkdir(parents=True, exist_ok=True)
        AUTOSTART.write_text(
            "[Desktop Entry]\nType=Application\nName=LID\nExec=python3 -m lider --minimized\n"
            "X-GNOME-Autostart-enabled=true\n"
        )
    elif AUTOSTART.exists():
        AUTOSTART.unlink()


def _browse_dir(parent: QWidget, edit: QLineEdit) -> None:
    path = QFileDialog.getExistingDirectory(parent, "Select a Folder", edit.text())
    if path:
        edit.setText(path)


class CategoryDialog(QDialog):
    """Category editor."""

    def __init__(self, cat: Category | None, parent=None):
        super().__init__(parent)
        self.setWindowTitle("LID categories")
        self.resize(dlu(322, 177).width(), 0)
        cat = cat or Category("", [], os.path.join(str(Path.home() / "Downloads"), "New"))
        g = QGridLayout(self)
        self.name = QLineEdit(cat.name)
        self.exts = QLineEdit(" ".join(cat.extensions))
        self.use_sites = QCheckBox("Automatically put in this category the files from the following sites:")
        self.use_sites.setChecked(bool(cat.sites))
        self.sites = QLineEdit(" ".join(cat.sites))
        self.folder = QLineEdit(cat.save_dir)
        self.remember = QCheckBox("Remember last save path")
        self.remember.setChecked(cat.remember_last)
        browse = QPushButton("Browse...")
        browse.clicked.connect(lambda: _browse_dir(self, self.folder))
        ok, cancel = QPushButton("OK"), QPushButton("Cancel")
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        g.addWidget(QLabel("Category name"), 0, 0)
        g.addWidget(self.name, 1, 0)
        g.addWidget(ok, 1, 1)
        g.addWidget(QLabel("Automatically put in this category the following file types:"), 2, 0)
        g.addWidget(self.exts, 3, 0)
        g.addWidget(cancel, 3, 1)
        g.addWidget(QLabel("Note: type file extensions separated by space (e.g. avi mpg mpeg)"), 4, 0, 1, 2)
        g.addWidget(separator(), 5, 0, 1, 2)
        g.addWidget(self.use_sites, 6, 0, 1, 2)
        g.addWidget(self.sites, 7, 0, 1, 2)
        g.addWidget(QLabel("Separate sites by spaces. You may use asterisk as a wildcard, e.g. *.example.com"), 8, 0, 1, 2)
        g.addWidget(separator(), 9, 0, 1, 2)
        g.addWidget(QLabel("Save future downloads of this category to the following folder:"), 10, 0, 1, 2)
        g.addWidget(self.folder, 11, 0, 1, 2)
        g.addWidget(self.remember, 12, 0)
        g.addWidget(browse, 12, 1)
        if cat.name == "General":
            self.name.setReadOnly(True)

    def category(self) -> Category:
        return Category(
            self.name.text().strip(),
            [e.lstrip(".").lower() for e in self.exts.text().split()],
            self.folder.text().strip(),
            self.sites.text().split() if self.use_sites.isChecked() else [],
            self.remember.isChecked(),
        )

    def accept(self):
        if self.name.text().strip() and self.folder.text().strip():
            super().accept()


class ServerLimitDialog(QDialog):
    """Per-server connection limit."""

    def __init__(self, pattern: str = "", n: int = 2, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Max. connections number for a server")
        g = QGridLayout(self)
        self.server = QLineEdit(pattern)
        self.conns = QComboBox()
        self.conns.addItems([str(c) for c in CONNECTION_CHOICES])
        self.conns.setCurrentText(str(n))
        g.addWidget(QLabel("Server"), 0, 0)
        g.addWidget(self.server, 0, 1)
        g.addWidget(QLabel("You may use asterisk as a wildcard pattern"), 1, 1)
        g.addWidget(QLabel("Max. connections number"), 2, 0)
        g.addWidget(self.conns, 2, 1)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.accept)
        bb.rejected.connect(self.reject)
        g.addWidget(bb, 3, 0, 1, 2)

    def value(self) -> tuple[str, int]:
        return self.server.text().strip(), int(self.conns.currentText())


class SpeedLimiterDialog(QDialog):
    """Speed Limiter settings."""

    def __init__(self, settings: Settings, parent=None):
        super().__init__(parent)
        self.settings = settings
        self.setWindowTitle("Speed Limiter settings")
        g = QGridLayout(self)
        self.kbps = QSpinBox()
        self.kbps.setRange(1, 10_000_000)
        self.kbps.setValue(settings.speed_limit_kbps)
        note = QLabel("Note: Internet servers may break connection when the speed is very low. "
                      "Please don't set the speed limit too low.")
        note.setWordWrap(True)
        self.on_startup = QCheckBox("Always turn on Speed Limiter on LID startup")
        self.on_startup.setChecked(settings.speed_limiter_on_startup)
        g.addWidget(QLabel("Maximum download speed for one file"), 0, 0)
        g.addWidget(self.kbps, 0, 1)
        g.addWidget(QLabel("KBytes/sec"), 0, 2)
        g.addWidget(note, 1, 0, 1, 3)
        g.addWidget(self.on_startup, 2, 0, 1, 3)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.accept)
        bb.rejected.connect(self.reject)
        g.addWidget(bb, 3, 0, 1, 3)

    def accept(self):
        self.settings.speed_limit_kbps = self.kbps.value()
        self.settings.speed_limiter_on_startup = self.on_startup.isChecked()
        super().accept()


class ColumnsDialog(QDialog):
    """Column chooser for the download list."""

    def __init__(self, settings: Settings, parent=None):
        super().__init__(parent)
        self.settings = settings
        self.setWindowTitle("Columns")
        titles = {k: t for k, t, _, _ in DEFAULT_COLUMNS}
        lay = QVBoxLayout(self)
        intro = QLabel("Check the columns that you would like visible in this list. Use the Move Up and Move Down "
                       "buttons to reorder the columns however you like.")
        intro.setWordWrap(True)
        lay.addWidget(intro)
        row = QHBoxLayout()
        self.list = QListWidget()
        for key, visible, width in settings.columns:
            item = QListWidgetItem(titles.get(key, key))
            item.setData(Qt.UserRole, [key, width])
            item.setFlags(item.flags() | Qt.ItemIsUserCheckable)
            item.setCheckState(Qt.Checked if visible else Qt.Unchecked)
            self.list.addItem(item)
        row.addWidget(self.list, 1)
        side = QVBoxLayout()
        for text, slot in (("Move Up", lambda: self._move(-1)), ("Move Down", lambda: self._move(1)),
                           ("Show", lambda: self._check(True)), ("Hide", lambda: self._check(False)),
                           ("Reset", self._reset)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            side.addWidget(b)
        side.addStretch(1)
        row.addLayout(side)
        lay.addLayout(row)
        w = QHBoxLayout()
        self.width = QSpinBox()
        self.width.setRange(10, 2000)
        w.addWidget(QLabel("The selected column should be"))
        w.addWidget(self.width)
        w.addWidget(QLabel("pixels wide"))
        w.addStretch(1)
        lay.addLayout(w)
        lay.addWidget(separator())
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.accept)
        bb.rejected.connect(self.reject)
        lay.addWidget(bb)
        self.list.currentItemChanged.connect(self._selected)
        self.width.valueChanged.connect(self._width_changed)
        self.list.setCurrentRow(0)

    def _selected(self, item, _prev):
        if item:
            self.width.setValue(item.data(Qt.UserRole)[1])

    def _width_changed(self, v):
        item = self.list.currentItem()
        if item:
            key, _ = item.data(Qt.UserRole)
            item.setData(Qt.UserRole, [key, v])

    def _move(self, delta):
        r = self.list.currentRow()
        if 0 <= r + delta < self.list.count():
            item = self.list.takeItem(r)
            self.list.insertItem(r + delta, item)
            self.list.setCurrentRow(r + delta)

    def _check(self, on):
        item = self.list.currentItem()
        if item:
            item.setCheckState(Qt.Checked if on else Qt.Unchecked)

    def _reset(self):
        self.settings.columns = [[k, v, w] for k, _, v, w in DEFAULT_COLUMNS]
        super().accept()

    def accept(self):
        cols = []
        for i in range(self.list.count()):
            item = self.list.item(i)
            key, width = item.data(Qt.UserRole)
            cols.append([key, item.checkState() == Qt.Checked, width])
        if not any(c[1] for c in cols if c[0] == "filename"):
            for c in cols:
                if c[0] == "filename":
                    c[1] = True
        self.settings.columns = cols
        super().accept()


class OptionsDialog(QDialog):
    def __init__(self, settings: Settings, parent=None):
        super().__init__(parent)
        self.settings = settings
        self.categories = [Category(c.name, list(c.extensions), c.save_dir, list(c.sites), c.remember_last)
                           for c in settings.categories]
        self.server_limits = dict(settings.server_limits)
        self.setWindowTitle("LID options")
        self.resize(dlu(268, 257).width() + 40, dlu(268, 257).height() + 60)
        self.tabs = QTabWidget()
        self.tabs.addTab(self._general(), "General")
        self.tabs.addTab(self._file_types(), "File types")
        self.tabs.addTab(self._save_to(), "Save to")
        self.tabs.addTab(self._downloads(), "Downloads")
        self.tabs.addTab(self._connection(), "Connection")
        self.tabs.addTab(self._proxy(), "Proxy / Socks")
        lay = QVBoxLayout(self)
        lay.addWidget(self.tabs)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.accepted.connect(self.accept)
        bb.rejected.connect(self.reject)
        lay.addWidget(bb)

    # -- pages
    def _page(self, icon_name: str, title: str) -> tuple[QWidget, QVBoxLayout]:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.addWidget(page_header(icon_name, title))
        lay.addWidget(separator())
        return w, lay

    def _general(self):
        w, lay = self._page("general", "Desktop Integration")
        s = self.settings
        self.launch = QCheckBox("Launch LID on startup")
        self.launch.setChecked(s.launch_on_startup or AUTOSTART.exists())
        self.clipboard = QCheckBox("Automatically start downloading of URLs placed to clipboard")
        self.clipboard.setChecked(s.monitor_clipboard)
        lay.addWidget(self.launch)
        lay.addWidget(self.clipboard)
        box = QGroupBox("Browser integration")
        b = QVBoxLayout(box)
        self.browser_capture = QCheckBox("Capture downloads from browsers (needs the LID extension)")
        self.browser_capture.setChecked(s.browser_capture)
        b.addWidget(self.browser_capture)
        note = QLabel("The extension takes over downloads of the file types on the File types tab, and adds "
                      "\"Download with LID\", \"Download all links with LID\" and a YouTube button.")
        note.setWordWrap(True)
        b.addWidget(note)
        row = QHBoxLayout()
        setup = QPushButton("Set up browsers...")
        setup.clicked.connect(self._browsers)
        row.addWidget(setup)
        row.addStretch(1)
        b.addLayout(row)
        lay.addSpacing(8)
        lay.addWidget(box)
        lay.addStretch(1)
        return w

    def _browsers(self):
        from .browsers import BrowserIntegrationDialog

        BrowserIntegrationDialog(self).exec()

    def _file_types(self):
        w, lay = self._page("filetypes", "Downloaded file types")
        lay.addWidget(QLabel("Automatically start downloading the following file types:"))
        self.types = QPlainTextEdit(self.settings.auto_capture_types)
        self.types.setFixedHeight(dlu(0, 36).height() + 10)
        lay.addWidget(self.types)
        row = QHBoxLayout()
        row.addStretch(1)
        default = QPushButton("Default")
        default.clicked.connect(lambda: self.types.setPlainText(DEFAULT_FILE_TYPES))
        row.addWidget(default)
        lay.addLayout(row)
        lay.addWidget(QLabel("Don't start downloading automatically from the following sites:"))
        self.no_sites = QPlainTextEdit(self.settings.no_auto_sites)
        self.no_sites.setFixedHeight(dlu(0, 30).height() + 10)
        lay.addWidget(self.no_sites)
        lay.addWidget(QLabel("(separate names by spaces)"))
        lay.addStretch(1)
        return w

    def _save_to(self):
        w, lay = self._page("saveto", "Categories, file types, folders")
        box = QGroupBox("Save To...")
        g = QGridLayout(box)
        self.cat_combo = QComboBox()
        new, edit = QPushButton("New"), QPushButton("Edit...")
        new.clicked.connect(self._new_category)
        edit.clicked.connect(self._edit_category)
        self.cat_types_label = QLabel()
        self.cat_types = QLineEdit()
        self.cat_dir_label = QLabel()
        self.cat_dir = QLineEdit()
        browse = QPushButton("Browse")
        browse.clicked.connect(lambda: _browse_dir(self, self.cat_dir))
        g.addWidget(QLabel("Category"), 0, 0)
        g.addWidget(self.cat_combo, 1, 0)
        g.addWidget(new, 0, 1)
        g.addWidget(edit, 1, 1)
        g.addWidget(self.cat_types_label, 2, 0, 1, 2)
        g.addWidget(self.cat_types, 3, 0, 1, 2)
        g.addWidget(self.cat_dir_label, 4, 0, 1, 2)
        g.addWidget(self.cat_dir, 5, 0)
        g.addWidget(browse, 5, 1)
        g.setColumnStretch(0, 1)
        lay.addWidget(box)
        tmp = QGroupBox("Temporary directory")
        t = QGridLayout(tmp)
        self.temp_dir = QLineEdit(self.settings.temp_dir)
        tb = QPushButton("Browse")
        tb.clicked.connect(lambda: _browse_dir(self, self.temp_dir))
        t.addWidget(self.temp_dir, 0, 0)
        t.addWidget(tb, 0, 1)
        note = QLabel("Temporary directory is required for storing file parts during downloading. "
                      "The completed file is moved to its category folder.")
        note.setWordWrap(True)
        t.addWidget(note, 1, 0, 1, 2)
        lay.addWidget(tmp)
        lay.addStretch(1)
        self._cat_index = -1
        self.cat_combo.currentIndexChanged.connect(self._show_category)
        self._reload_categories()
        return w

    def _reload_categories(self, select: str | None = None):
        self.cat_combo.blockSignals(True)
        self.cat_combo.clear()
        self.cat_combo.addItems([c.name for c in self.categories])
        self.cat_combo.blockSignals(False)
        self._cat_index = -1
        idx = self.cat_combo.findText(select) if select else 0
        self.cat_combo.setCurrentIndex(max(0, idx))
        self._show_category(self.cat_combo.currentIndex())

    def _store_category(self):
        if 0 <= self._cat_index < len(self.categories):
            c = self.categories[self._cat_index]
            c.extensions = [e.lstrip(".").lower() for e in self.cat_types.text().split()]
            c.save_dir = self.cat_dir.text().strip() or c.save_dir

    def _show_category(self, index: int):
        self._store_category()
        self._cat_index = index
        if index < 0:
            return
        c = self.categories[index]
        self.cat_types_label.setText(f'Automatically put in "{c.name}" category the following file types:')
        self.cat_types.setText(" ".join(c.extensions))
        self.cat_dir_label.setText(f'Default download directory for "{c.name}" category')
        self.cat_dir.setText(c.save_dir)

    def _new_category(self):
        self._store_category()
        dlg = CategoryDialog(None, self)
        if dlg.exec():
            cat = dlg.category()
            if all(c.name != cat.name for c in self.categories):
                self.categories.append(cat)
                self._reload_categories(cat.name)

    def _edit_category(self):
        self._store_category()
        i = self.cat_combo.currentIndex()
        dlg = CategoryDialog(self.categories[i], self)
        if dlg.exec():
            self.categories[i] = dlg.category()
            self._reload_categories(self.categories[i].name)

    def _downloads(self):
        w, lay = self._page("downloads", "Default download settings")
        s = self.settings
        self.show_progress = QCheckBox('Show "Download progress" dialog')
        self.show_progress.setChecked(s.show_progress_dialog)
        self.show_start = QCheckBox("Show start download dialog")
        self.show_start.setChecked(s.show_start_dialog)
        self.only_queue = QCheckBox("Do not start downloading, only add files to the queue")
        self.only_queue.setChecked(s.only_add_to_queue)
        self.show_complete = QCheckBox("Show download complete dialog")
        self.show_complete.setChecked(s.show_complete_dialog)
        self.duplicate = QComboBox()
        self.duplicate.addItem("Show the dialog and ask me", "ask")
        self.duplicate.addItem("Add the duplicate with a numbered file name", "number")
        self.duplicate.addItem("Add the duplicate and overwrite the existing file", "overwrite")
        self.duplicate.addItem("If existing file is complete, show download complete dialog; otherwise resume it",
                               "complete")
        self.duplicate.setCurrentIndex(max(0, self.duplicate.findData(s.duplicate_action)))
        self.user_agent = QLineEdit(s.user_agent)
        lay.addWidget(self.show_progress)
        lay.addWidget(self.show_start)
        ind = QHBoxLayout()
        ind.addSpacing(20)
        ind.addWidget(self.only_queue)
        lay.addLayout(ind)
        lay.addWidget(self.show_complete)
        lay.addWidget(separator())
        lay.addWidget(QLabel("If a duplicate download link is added:"))
        lay.addWidget(self.duplicate)
        lay.addWidget(separator())
        lay.addWidget(QLabel("User-Agent for manually added downloads:"))
        lay.addWidget(self.user_agent)
        lay.addStretch(1)
        self.show_start.toggled.connect(self.only_queue.setEnabled)
        self.only_queue.setEnabled(s.show_start_dialog)
        return w

    def _connection(self):
        w, lay = self._page("connection", "Connections and Limits")
        box = QGroupBox("Max. connections number")
        g = QGridLayout(box)
        self.max_conn = QComboBox()
        self.max_conn.addItems([str(c) for c in CONNECTION_CHOICES])
        self.max_conn.setCurrentText(str(self.settings.max_connections))
        g.addWidget(QLabel("Default max. conn. number"), 0, 0)
        g.addWidget(self.max_conn, 0, 1)
        g.addWidget(separator(), 1, 0, 1, 2)
        g.addWidget(QLabel("Exceptions:"), 2, 0, 1, 2)
        self.exceptions = QTreeWidget()
        self.exceptions.setHeaderLabels(["Server", "Max. connections"])
        self.exceptions.setRootIsDecorated(False)
        self.exceptions.setSelectionMode(QAbstractItemView.SingleSelection)
        g.addWidget(self.exceptions, 3, 0)
        side = QVBoxLayout()
        for text, slot in (("New", self._new_limit), ("Delete", self._delete_limit), ("Edit", self._edit_limit)):
            b = QPushButton(text)
            b.clicked.connect(slot)
            side.addWidget(b)
        side.addStretch(1)
        g.addLayout(side, 3, 1)
        lay.addWidget(box)
        retry = QHBoxLayout()
        self.retries = QSpinBox()
        self.retries.setRange(0, 100)
        self.retries.setValue(self.settings.retries)
        retry.addWidget(QLabel("Number of reconnection attempts for each connection"))
        retry.addWidget(self.retries)
        retry.addStretch(1)
        lay.addLayout(retry)
        lay.addStretch(1)
        self._fill_limits()
        return w

    def _fill_limits(self):
        self.exceptions.clear()
        for pattern, n in sorted(self.server_limits.items()):
            QTreeWidgetItem(self.exceptions, [pattern, str(n)])

    def _new_limit(self):
        dlg = ServerLimitDialog(parent=self)
        if dlg.exec():
            p, n = dlg.value()
            if p:
                self.server_limits[p] = n
                self._fill_limits()

    def _edit_limit(self):
        item = self.exceptions.currentItem()
        if not item:
            return
        dlg = ServerLimitDialog(item.text(0), int(item.text(1)), self)
        if dlg.exec():
            p, n = dlg.value()
            self.server_limits.pop(item.text(0), None)
            if p:
                self.server_limits[p] = n
            self._fill_limits()

    def _delete_limit(self):
        item = self.exceptions.currentItem()
        if item:
            self.server_limits.pop(item.text(0), None)
            self._fill_limits()

    def _proxy(self):
        w, lay = self._page("proxy", "Proxy")
        self.use_proxy = QCheckBox("Use proxy")
        self.use_proxy.setChecked(bool(self.settings.proxy))
        self.proxy = QLineEdit(self.settings.proxy or "http://127.0.0.1:8080")
        self.proxy.setEnabled(self.use_proxy.isChecked())
        self.use_proxy.toggled.connect(self.proxy.setEnabled)
        lay.addWidget(self.use_proxy)
        row = QHBoxLayout()
        row.addWidget(QLabel("Proxy address"))
        row.addWidget(self.proxy, 1)
        lay.addLayout(row)
        lay.addWidget(QLabel("Format: http://[user:password@]host:port   (used for HTTP and HTTPS)"))
        lay.addStretch(1)
        return w

    # -- apply
    def accept(self):
        self._store_category()
        s = self.settings
        s.launch_on_startup = self.launch.isChecked()
        set_autostart(s.launch_on_startup)
        s.monitor_clipboard = self.clipboard.isChecked()
        s.browser_capture = self.browser_capture.isChecked()
        s.auto_capture_types = " ".join(self.types.toPlainText().split())
        s.no_auto_sites = " ".join(self.no_sites.toPlainText().split())
        s.categories = self.categories
        s.temp_dir = self.temp_dir.text().strip() or s.temp_dir
        s.show_progress_dialog = self.show_progress.isChecked()
        s.show_start_dialog = self.show_start.isChecked()
        s.only_add_to_queue = self.only_queue.isChecked()
        s.show_complete_dialog = self.show_complete.isChecked()
        s.duplicate_action = self.duplicate.currentData()
        s.user_agent = self.user_agent.text().strip() or s.user_agent
        s.max_connections = int(self.max_conn.currentText())
        s.server_limits = self.server_limits
        s.retries = self.retries.value()
        s.proxy = self.proxy.text().strip() if self.use_proxy.isChecked() else ""
        super().accept()
