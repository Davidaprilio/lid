"""Options dialog (General, File types, Save to, Downloads, Connection, Proxy) and its helper dialogs."""

from __future__ import annotations

import os
from pathlib import Path

from PySide6.QtCore import QSize, Qt
from PySide6.QtGui import QColor, QFont, QPainter, QPainterPath
from PySide6.QtWidgets import (
    QAbstractItemView,
    QButtonGroup,
    QFontDialog,
    QFrame,
    QRadioButton,
    QToolButton,
    QCheckBox,
    QComboBox,
    QDialog,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPlainTextEdit,
    QPushButton,
    QSizePolicy,
    QSpinBox,
    QTabWidget,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ...core.settings import CONNECTION_CHOICES, DEFAULT_COLUMNS, DEFAULT_FILE_TYPES, Category, Settings
from .. import theme
from ... import browser, desktop
from ..icons import category_icon, icon
from ..util import page_header, separator
from ..widgets import (TagInput, caption, extension_tag, form_label, form_row, mono_font, note, panel, pill_label,
                       primary, shell, site_tag, small_header)

def _browse_dir(parent: QWidget, edit: QLineEdit) -> None:
    path = QFileDialog.getExistingDirectory(parent, "Select a Folder", edit.text())
    if path:
        edit.setText(path)


def _ok_cancel(dialog: QDialog, footer: QHBoxLayout) -> None:
    ok, cancel = primary(QPushButton("OK")), QPushButton("Cancel")
    ok.setDefault(True)
    ok.clicked.connect(dialog.accept)
    cancel.clicked.connect(dialog.reject)
    footer.addStretch(1)
    footer.addWidget(cancel)
    footer.addWidget(ok)


class CategoryDialog(QDialog):
    """Category editor: file types and sites are removable badges."""

    def __init__(self, cat: Category | None, parent=None):
        super().__init__(parent)
        self.setWindowTitle("LID categories")
        self.setMinimumWidth(640)
        new = cat is None
        cat = cat or Category("", [], os.path.join(str(Path.home() / "Downloads"), "New"))
        lay, footer = shell(self, category_icon(cat.name) if cat.name else "folder", "LID categories",
                            "New category" if new else f"Edit the {cat.name} category", spacing=9, tile=True)
        self.name = QLineEdit(cat.name)
        lay.addLayout(form_row("Category name", self.name, width=110))
        lay.addSpacing(2)
        lay.addWidget(QLabel("Automatically put in this category the following file types:"))
        self.exts = TagInput(cat.extensions, "Add a type, e.g. mkv", extension_tag, "types")
        lay.addWidget(self.exts)
        lay.addWidget(self.exts.meta_row())
        self.use_sites = QCheckBox("Automatically put in this category the files from these sites:")
        self.use_sites.setChecked(bool(cat.sites))
        self.sites = TagInput(cat.sites, "Add a site, e.g. *.example.com", site_tag, "sites")
        self.sites.setEnabled(self.use_sites.isChecked())
        self.use_sites.toggled.connect(self.sites.setEnabled)
        lay.addWidget(self.use_sites)
        lay.addWidget(self.sites)
        lay.addWidget(QLabel("Save future downloads of this category to:"))
        row = QHBoxLayout()
        self.folder = QLineEdit(cat.save_dir)
        self.folder.setFont(mono_font(self.folder.font()))
        browse = QPushButton("Browse...")
        browse.clicked.connect(lambda: _browse_dir(self, self.folder))
        row.addWidget(self.folder, 1)
        row.addWidget(browse)
        lay.addLayout(row)
        self.remember = QCheckBox("Remember last save path")
        self.remember.setChecked(cat.remember_last)
        lay.addWidget(self.remember)
        _ok_cancel(self, footer)
        if cat.name == "General":
            self.name.setReadOnly(True)

    def category(self) -> Category:
        return Category(
            self.name.text().strip(),
            self.exts.values(),
            self.folder.text().strip(),
            self.sites.values() if self.use_sites.isChecked() else [],
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
        self.setMinimumWidth(480)
        body, footer = shell(self, spacing=8)
        self.server = QLineEdit(pattern)
        self.server.setFont(mono_font(self.server.font()))
        self.conns = QComboBox()
        self.conns.addItems([str(c) for c in CONNECTION_CHOICES])
        self.conns.setCurrentText(str(n))
        self.conns.setFixedWidth(90)
        body.addLayout(form_row("Server", self.server, width=170))
        hint = QLabel("You may use an asterisk as a wildcard.")
        hint.setObjectName("faint")
        hint.setContentsMargins(180, 0, 0, 4)
        body.addWidget(hint)
        body.addLayout(form_row("Max. connections number", self.conns, QWidget(), width=170))
        _ok_cancel(self, footer)

    def value(self) -> tuple[str, int]:
        return self.server.text().strip(), int(self.conns.currentText())


class SpeedLimiterDialog(QDialog):
    """Speed Limiter settings."""

    def __init__(self, settings: Settings, parent=None):
        super().__init__(parent)
        self.settings = settings
        self.setWindowTitle("Speed Limiter settings")
        self.setMinimumWidth(520)
        body, footer = shell(self, "speed", "Speed Limiter settings", "Applies to all downloads while the limiter is on",
                             spacing=12, badge="accent")
        self.kbps = QSpinBox()
        self.kbps.setRange(1, 10_000_000)
        self.kbps.setValue(settings.speed_limit_kbps)
        self.kbps.setFont(mono_font(self.kbps.font()))
        self.kbps.setFixedWidth(120)
        body.addLayout(form_row("Maximum speed", self.kbps, form_label("KBytes/sec"), QWidget(), width=110))
        body.addWidget(note("Note: servers may close the connection when the speed is very low.", boxed=True))
        self.on_startup = QCheckBox("Always turn on Speed Limiter on LID startup")
        self.on_startup.setChecked(settings.speed_limiter_on_startup)
        body.addWidget(self.on_startup)
        _ok_cancel(self, footer)

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
        self.setMinimumWidth(440)
        titles = {k: t for k, t, _, _ in DEFAULT_COLUMNS}
        lay, footer = shell(self, spacing=12)
        intro = QLabel("Check the columns to show. Drag or use the buttons to change the order.")
        intro.setObjectName("muted")
        intro.setWordWrap(True)
        lay.addWidget(intro)
        row = QHBoxLayout()
        row.setSpacing(10)
        self.list = QListWidget()
        self.list.setObjectName("checkList")
        self.list.setDragDropMode(QAbstractItemView.InternalMove)
        self.list.setDefaultDropAction(Qt.MoveAction)
        for key, visible, width in settings.columns:
            item = QListWidgetItem(titles.get(key, key))
            item.setData(Qt.UserRole, [key, width])
            item.setFlags(item.flags() | Qt.ItemIsUserCheckable)
            item.setCheckState(Qt.Checked if visible else Qt.Unchecked)
            item.setSizeHint(QSize(0, 30))
            self.list.addItem(item)
        self.list.setMinimumHeight(12 * 30 + 16)
        row.addWidget(self.list, 1)
        side = QVBoxLayout()
        side.setSpacing(6)
        for text, slot in (("Move up", lambda: self._move(-1)), ("Move down", lambda: self._move(1))):
            b = QPushButton(text)
            b.clicked.connect(slot)
            side.addWidget(b)
        side.addStretch(1)
        reset = QPushButton("Reset")
        reset.setObjectName("ghostBtn")
        reset.setToolTip("Back to the default columns")
        reset.clicked.connect(self._reset)
        side.addWidget(reset)
        row.addLayout(side)
        lay.addLayout(row)
        self.column_width = QSpinBox()
        self.column_width.setRange(10, 2000)
        self.column_width.setFont(mono_font(self.column_width.font()))
        self.column_width.setFixedWidth(90)
        lay.addLayout(form_row("Selected column width", self.column_width, form_label("px"), QWidget(), width=150))
        _ok_cancel(self, footer)
        self.list.currentItemChanged.connect(self._selected)
        self.column_width.valueChanged.connect(self._width_changed)
        self.list.setCurrentRow(0)

    def _selected(self, item, _prev):
        if item:
            self.column_width.setValue(item.data(Qt.UserRole)[1])

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
        self.resize(720, 712)
        self.tabs = QTabWidget()
        self.tabs.setObjectName("optionsTabs")
        self.tabs.addTab(self._general(), "General")
        self.tabs.addTab(self._file_types(), "File types")
        self.tabs.addTab(self._save_to(), "Save to")
        self.tabs.addTab(self._downloads(), "Downloads")
        self.tabs.addTab(self._connection(), "Connection")
        self.tabs.addTab(self._proxy(), "Proxy / Socks")
        self.tabs.addTab(self._appearance(), "Appearance")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addWidget(self.tabs, 1)
        bar = QFrame()
        bar.setObjectName("footer")
        footer = QHBoxLayout(bar)
        footer.setContentsMargins(16, 12, 16, 12)
        footer.setSpacing(8)
        _ok_cancel(self, footer)
        lay.addWidget(bar)

    # -- pages
    def _page(self, icon_name: str, title: str) -> tuple[QWidget, QVBoxLayout]:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.setContentsMargins(18, 14, 18, 14)
        lay.setSpacing(10)
        lay.addWidget(page_header(icon_name, title))
        lay.addWidget(separator())
        lay.addSpacing(4)
        return w, lay

    @staticmethod
    def _group(title: str) -> tuple[QFrame, QVBoxLayout]:
        box, lay = panel()
        lay.addWidget(caption(title))
        return box, lay

    def _general(self):
        w, lay = self._page("general", "Desktop Integration")
        s = self.settings
        self.launch = QCheckBox("Launch LID on startup")
        self.launch.setChecked(desktop.autostart_enabled())
        self.menu = QCheckBox("Show LID in the application menu")
        if desktop.system_installed():  # the package's own entry
            self.menu.setChecked(True)
            self.menu.setEnabled(False)
            self.menu.setToolTip("Added by the installed LID package")
        else:
            self.menu.setChecked(desktop.in_menu())
        self.clipboard = QCheckBox("Automatically start downloading of URLs placed to clipboard")
        self.clipboard.setChecked(s.monitor_clipboard)
        lay.addWidget(self.launch)
        lay.addWidget(self.menu)
        lay.addWidget(self.clipboard)
        box, b = self._group("Browser integration")
        self.browser_capture = QCheckBox("Capture downloads from browsers (needs the LID extension)")
        self.browser_capture.setChecked(s.browser_capture)
        b.addWidget(self.browser_capture)
        b.addWidget(note("The extension takes over downloads of the file types on the File types tab, and adds "
                         "\u201cDownload with LID\u201d, \u201cDownload all links with LID\u201d and a YouTube button."))
        row = QHBoxLayout()
        row.setSpacing(8)
        setup = QPushButton("Set up browsers...")
        setup.clicked.connect(self._browsers)
        row.addWidget(setup)
        connected = sum(1 for br in browser.BROWSERS if browser.is_installed(br))
        row.addWidget(pill_label(f"✓ {connected} browser{'s' if connected != 1 else ''} connected", "ok") if connected
                      else pill_label("Not set up yet", "muted"))
        row.addStretch(1)
        b.addLayout(row)
        lay.addWidget(box)
        lay.addStretch(1)
        return w

    def _browsers(self):
        from .browsers import BrowserIntegrationDialog

        BrowserIntegrationDialog(self).exec()

    def _file_types(self):
        w, lay = self._page("filetypes", "Downloaded file types")
        lay.addWidget(QLabel("Automatically start downloading the following file types:"))
        self.types = TagInput(self.settings.auto_capture_types, "Add a file type…", extension_tag, "types")
        lay.addWidget(self.types)
        lay.addWidget(self.types.meta_row())
        default = QPushButton("Default")
        default.setToolTip("Back to the default file types")
        default.clicked.connect(lambda: self.types.set_values(DEFAULT_FILE_TYPES))
        lay.addWidget(default, 0, Qt.AlignRight)
        lay.addSpacing(4)
        lay.addWidget(QLabel("Don't start downloading automatically from the following sites:"))
        self.no_sites = TagInput(self.settings.no_auto_sites, "Add a site, e.g. *.example.com", site_tag, "sites")
        lay.addWidget(self.no_sites)
        lay.addStretch(1)
        return w

    def _save_to(self):
        w, lay = self._page("saveto", "Categories, file types, folders")
        box, g = self._group("Save to...")
        self.cat_combo = QComboBox()
        new, edit = QPushButton("New"), QPushButton("Edit...")
        new.clicked.connect(self._new_category)
        edit.clicked.connect(self._edit_category)
        g.addLayout(form_row("Category", self.cat_combo, new, edit, width=64, stretch_last=False))
        self.cat_combo.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Fixed)
        self.cat_types_label = form_label("")
        self.cat_types = TagInput([], "Add a type…", extension_tag, "types")
        self.cat_dir_label = form_label("")
        self.cat_dir = QLineEdit()
        self.cat_dir.setFont(mono_font(self.cat_dir.font()))
        browse = QPushButton("Browse")
        browse.clicked.connect(lambda: _browse_dir(self, self.cat_dir))
        g.addWidget(self.cat_types_label)
        g.addWidget(self.cat_types)
        g.addWidget(self.cat_dir_label)
        row = QHBoxLayout()
        row.addWidget(self.cat_dir, 1)
        row.addWidget(browse)
        g.addLayout(row)
        lay.addWidget(box)
        tmp, t = self._group("Temporary directory")
        self.temp_dir = QLineEdit(self.settings.temp_dir)
        self.temp_dir.setFont(mono_font(self.temp_dir.font()))
        tb = QPushButton("Browse")
        tb.clicked.connect(lambda: _browse_dir(self, self.temp_dir))
        row = QHBoxLayout()
        row.addWidget(self.temp_dir, 1)
        row.addWidget(tb)
        t.addLayout(row)
        hint = QLabel("Parts are stored here while downloading. The finished file moves to its category folder.")
        hint.setObjectName("faint")
        hint.setWordWrap(True)
        t.addWidget(hint)
        lay.addWidget(tmp)
        lay.addStretch(1)
        self._cat_index = -1
        self.cat_combo.currentIndexChanged.connect(self._show_category)
        self._reload_categories()
        return w

    def _reload_categories(self, select: str | None = None):
        self.cat_combo.blockSignals(True)
        self.cat_combo.clear()
        for c in self.categories:
            self.cat_combo.addItem(icon(category_icon(c.name)), c.name)
        self.cat_combo.blockSignals(False)
        self._cat_index = -1
        idx = self.cat_combo.findText(select) if select else 0
        self.cat_combo.setCurrentIndex(max(0, idx))
        self._show_category(self.cat_combo.currentIndex())

    def _store_category(self):
        if 0 <= self._cat_index < len(self.categories):
            c = self.categories[self._cat_index]
            c.extensions = self.cat_types.values()
            c.save_dir = self.cat_dir.text().strip() or c.save_dir

    def _show_category(self, index: int):
        self._store_category()
        self._cat_index = index
        if index < 0:
            return
        c = self.categories[index]
        self.cat_types_label.setText(f'Automatically put in \u201c{c.name}\u201d the following file types:')
        self.cat_types.set_values(c.extensions)
        self.cat_dir_label.setText(f'Default download directory for \u201c{c.name}\u201d')
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
        self.duplicate.setSizeAdjustPolicy(QComboBox.AdjustToMinimumContentsLengthWithIcon)
        self.duplicate.setMinimumContentsLength(24)
        self.user_agent = QLineEdit(s.user_agent)
        self.user_agent.setFont(mono_font(self.user_agent.font()))
        self.user_agent.setCursorPosition(0)
        lay.addWidget(self.show_progress)
        lay.addWidget(self.show_start)
        ind = QHBoxLayout()
        ind.addSpacing(24)
        ind.addWidget(self.only_queue)
        lay.addLayout(ind)
        lay.addWidget(self.show_complete)
        lay.addSpacing(6)
        lay.addLayout(form_row("If a duplicate download link is added", self.duplicate, width=240))
        lay.addLayout(form_row("User-Agent for manual downloads", self.user_agent, width=240))
        lay.addSpacing(10)
        lay.addWidget(page_header("checksum", "Checksum verification"))
        lay.addWidget(separator())
        self.verify_checksum = QCheckBox("Verify the checksum when a download finishes (when one was entered)")
        self.verify_checksum.setChecked(s.verify_checksum)
        lay.addWidget(self.verify_checksum)
        why = QLabel("Reads the whole file once, after its parts are merged. Large files take a few seconds; "
                     "the list shows \u201cVerifying\u201d.")
        why.setObjectName("faint")
        why.setWordWrap(True)
        why.setContentsMargins(24, 0, 0, 0)
        lay.addWidget(why)
        self.checksum_clipboard = QCheckBox("Fill in the checksum from the clipboard when it holds a hash")
        self.checksum_clipboard.setChecked(s.checksum_from_clipboard)
        lay.addWidget(self.checksum_clipboard)
        self.checksum_mismatch = QComboBox()
        self.checksum_mismatch.addItem("Ask me — show the mismatch dialog", "ask")
        self.checksum_mismatch.addItem("Keep the file, mark it \u201cChecksum mismatch\u201d", "keep")
        self.checksum_mismatch.addItem("Delete the file and download it again", "redownload")
        self.checksum_mismatch.setCurrentIndex(max(0, self.checksum_mismatch.findData(s.checksum_mismatch)))
        lay.addLayout(form_row("If the checksum doesn't match", self.checksum_mismatch, width=240))
        lay.addStretch(1)
        self.show_start.toggled.connect(self.only_queue.setEnabled)
        self.only_queue.setEnabled(s.show_start_dialog)
        return w

    def _connection(self):
        w, lay = self._page("connection", "Connections and Limits")
        box, g = self._group("Max. connections number")
        self.max_conn = QComboBox()
        self.max_conn.addItems([str(c) for c in CONNECTION_CHOICES])
        self.max_conn.setCurrentText(str(self.settings.max_connections))
        self.max_conn.setFixedWidth(80)
        g.addLayout(form_row("Default max. conn. number", self.max_conn, QWidget(), width=180))
        g.addWidget(form_label("Exceptions"))
        self.exceptions = QTreeWidget()
        self.exceptions.setHeaderLabels(["SERVER", "MAX. CONNECTIONS"])
        self.exceptions.setRootIsDecorated(False)
        self.exceptions.setAlternatingRowColors(True)
        self.exceptions.setSelectionMode(QAbstractItemView.SingleSelection)
        self.exceptions.setColumnWidth(0, 220)
        small_header(self.exceptions)
        ex = QHBoxLayout()
        ex.setSpacing(8)
        ex.addWidget(self.exceptions, 1)
        side = QVBoxLayout()
        side.setSpacing(6)
        for text, slot in (("New", self._new_limit), ("Delete", self._delete_limit), ("Edit", self._edit_limit)):
            b = QPushButton(text)
            b.setMinimumWidth(56)
            b.clicked.connect(slot)
            side.addWidget(b)
        side.addStretch(1)
        ex.addLayout(side)
        g.addLayout(ex)
        lay.addWidget(box)
        self.retries = QSpinBox()
        self.retries.setRange(0, 100)
        self.retries.setValue(self.settings.retries)
        self.retries.setFont(mono_font(self.retries.font()))
        self.retries.setFixedWidth(70)
        lay.addLayout(form_row("Reconnection attempts for each connection", self.retries, QWidget(), width=270))
        lay.addStretch(1)
        self._fill_limits()
        return w

    def _fill_limits(self):
        self.exceptions.clear()
        mono = mono_font(self.exceptions.font())
        for pattern, n in sorted(self.server_limits.items()):
            item = QTreeWidgetItem(self.exceptions, [pattern, str(n)])
            for col in (0, 1):
                item.setFont(col, mono)

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
        self.proxy.setFont(mono_font(self.proxy.font()))
        self.proxy.setEnabled(self.use_proxy.isChecked())
        self.use_proxy.toggled.connect(self.proxy.setEnabled)
        lay.addWidget(self.use_proxy)
        lay.addLayout(form_row("Proxy address", self.proxy, width=96))
        hint = QLabel("Format: http://[user:password@]host:port — used for HTTP and HTTPS.")
        hint.setObjectName("faint")
        lay.addWidget(hint)
        lay.addStretch(1)
        return w

    def _appearance(self):
        w, lay = self._page("general", "Appearance")
        s = self.settings
        lay.addWidget(caption("Theme"))
        cards = QHBoxLayout()
        self.theme_group = QButtonGroup(self)
        for key in theme.THEMES:
            card = _ThemeCard(key, self.theme_group)
            card.radio.setChecked(key == s.theme)
            cards.addWidget(card)
        lay.addLayout(cards)
        row = QHBoxLayout()
        self.follow_system = QCheckBox("Follow the system light / dark setting")
        self.follow_system.setChecked(s.follow_system)
        self.dark_theme = QComboBox()
        for key in theme.DARK_THEMES:
            self.dark_theme.addItem(theme.TITLES[key], key)
        self.dark_theme.setCurrentIndex(max(0, self.dark_theme.findData(s.dark_theme)))
        self.follow_system.toggled.connect(self.dark_theme.setEnabled)
        self.dark_theme.setEnabled(s.follow_system)
        row.addWidget(self.follow_system)
        row.addStretch(1)
        row.addWidget(QLabel("When dark, use"))
        row.addWidget(self.dark_theme)
        lay.addLayout(row)
        lay.addWidget(separator())
        grid = QGridLayout()
        self.density = _segmented([("Comfortable", "comfortable"), ("Compact", "compact")], s.density)
        self.labels = _segmented([("Icons and labels", True), ("Icons only", False)], s.toolbar_labels)
        grid.setVerticalSpacing(12)
        grid.addWidget(form_label("List density"), 0, 0)
        grid.addWidget(self.density, 0, 1, Qt.AlignLeft)
        grid.addWidget(form_label("Toolbar"), 1, 0)
        grid.addWidget(self.labels, 1, 1, Qt.AlignLeft)
        self.menubar = _segmented([("Native", "native"), ("In window", "custom"), ("Both", "both")], s.menubar)
        mb_row = QHBoxLayout()
        mb_row.addWidget(self.menubar)
        hint = QLabel("Native = desktop global menu" if theme.global_menu_available()
                      else "No global menu here: Native shows it in the window")
        hint.setObjectName("faint")
        mb_row.addWidget(hint, 1)
        grid.addWidget(form_label("Menu bar"), 2, 0)
        grid.addLayout(mb_row, 2, 1)
        self.list_font = s.list_font
        self.font_label = QLineEdit()
        self.font_label.setReadOnly(True)
        self.font_label.setFocusPolicy(Qt.NoFocus)
        self._show_font()
        change, reset = QPushButton("Change..."), QPushButton("Reset")
        reset.setObjectName("ghostBtn")
        change.clicked.connect(self._choose_font)
        reset.clicked.connect(lambda: (setattr(self, "list_font", ""), self._show_font()))
        fr = QHBoxLayout()
        fr.addWidget(self.font_label, 1)
        fr.addWidget(change)
        fr.addWidget(reset)
        grid.addWidget(form_label("List font"), 3, 0)
        grid.addLayout(fr, 3, 1)
        grid.setColumnMinimumWidth(0, 160)
        grid.setColumnStretch(1, 1)
        lay.addLayout(grid)
        lay.addWidget(separator())
        boxes = QGridLayout()
        self.view_boxes = {}
        for i, (key, text) in enumerate((("show_categories", "Show Categories panel"), ("show_details", "Show details pane (F9)"),
                                          ("show_statusbar", "Show status bar"), ("tray_icon", "Show icon in system tray"),
                                          ("close_to_tray", "Close button hides to tray"), ("start_minimized", "Start minimized to tray"))):
            cb = QCheckBox(text)
            cb.setChecked(getattr(s, key))
            self.view_boxes[key] = cb
            boxes.addWidget(cb, i // 2, i % 2)
        lay.addLayout(boxes)
        lay.addStretch(1)
        return w

    def _show_font(self):
        f = QFont()
        if not (self.list_font and f.fromString(self.list_font)):
            f = self.font()
            self.font_label.setText(f"System default · {f.pointSize()} pt")
        else:
            self.font_label.setText(f"{f.family()} · {f.pointSize()} pt")

    def _choose_font(self):
        f = QFont()
        if not (self.list_font and f.fromString(self.list_font)):
            f = self.font()
        ok, font = QFontDialog.getFont(f, self)
        if ok:
            self.list_font = font.toString()
            self._show_font()

    # -- apply
    def accept(self):
        self._store_category()
        s = self.settings
        s.launch_on_startup = self.launch.isChecked()
        try:  # only what changed: the files are left alone otherwise
            if s.launch_on_startup != desktop.autostart_enabled():
                desktop.set_autostart(s.launch_on_startup)
            if self.menu.isEnabled() and self.menu.isChecked() and not desktop.in_menu():
                desktop.add_to_menu()
            elif self.menu.isEnabled() and not self.menu.isChecked() and desktop.in_menu():
                desktop.remove_from_menu()
        except OSError:
            pass
        s.monitor_clipboard = self.clipboard.isChecked()
        s.browser_capture = self.browser_capture.isChecked()
        s.auto_capture_types = self.types.text()
        s.no_auto_sites = self.no_sites.text()
        s.categories = self.categories
        s.temp_dir = self.temp_dir.text().strip() or s.temp_dir
        s.show_progress_dialog = self.show_progress.isChecked()
        s.show_start_dialog = self.show_start.isChecked()
        s.only_add_to_queue = self.only_queue.isChecked()
        s.show_complete_dialog = self.show_complete.isChecked()
        s.duplicate_action = self.duplicate.currentData()
        s.user_agent = self.user_agent.text().strip() or s.user_agent
        s.verify_checksum = self.verify_checksum.isChecked()
        s.checksum_from_clipboard = self.checksum_clipboard.isChecked()
        s.checksum_mismatch = self.checksum_mismatch.currentData()
        s.max_connections = int(self.max_conn.currentText())
        s.server_limits = self.server_limits
        s.retries = self.retries.value()
        s.proxy = self.proxy.text().strip() if self.use_proxy.isChecked() else ""
        checked = self.theme_group.checkedButton()
        if checked is not None:
            s.theme = checked.property("theme")
        s.follow_system = self.follow_system.isChecked()
        s.dark_theme = self.dark_theme.currentData()
        s.density = self.density.value()
        s.toolbar_labels = self.labels.value()
        s.menubar = self.menubar.value()
        s.list_font = self.list_font
        for key, cb in self.view_boxes.items():
            setattr(s, key, cb.isChecked())
        super().accept()


class _Segmented(QFrame):
    def __init__(self, items, current):
        super().__init__()
        self.setObjectName("segmented")
        lay = QHBoxLayout(self)
        lay.setContentsMargins(3, 3, 3, 3)
        lay.setSpacing(0)
        self.group = QButtonGroup(self)
        for text, value in items:
            b = QToolButton()
            b.setObjectName("seg")
            b.setText(text)
            b.setCheckable(True)
            b.setProperty("value", value)
            b.setChecked(value == current)
            self.group.addButton(b)
            lay.addWidget(b)

    def value(self):
        b = self.group.checkedButton()
        return b.property("value") if b else None


def _segmented(items, current) -> _Segmented:
    return _Segmented(items, current)


class _ThemePreview(QWidget):
    """A tiny drawing of the main window in a theme's colours."""

    def __init__(self, key: str):
        super().__init__()
        self.key = key
        self.setFixedHeight(78)

    def paintEvent(self, _e):
        c = theme.THEMES[self.key]
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        r = self.rect().adjusted(0, 0, -1, -1)
        path = QPainterPath()
        path.addRoundedRect(r, 8, 8)
        p.fillPath(path, QColor(c["pane"]))
        p.setClipPath(path)
        w, h = r.width(), r.height()
        p.fillRect(0, 0, w, 16, QColor(c["win"]))
        bar = lambda x, y, bw, bh, col: p.fillRect(x, y, bw, bh, QColor(col))  # noqa: E731
        bar(6, 5, 22, 7, c["ac"])
        bar(32, 5, 14, 7, c["track"])
        bar(50, 5, 14, 7, c["track"])
        side = int(w * .3)
        p.fillRect(0, 16, side, h - 16, QColor(c["win"]))
        bar(5, 22, side - 10, 6, c["acsel"])
        bar(5, 32, int((side - 10) * .7), 6, c["track"])
        bar(5, 42, int((side - 10) * .8), 6, c["track"])
        x0 = side + 6
        bar(x0, 22, w - x0 - 6, 7, c["acsel"])
        bar(x0, 22, 2, 7, c["ac"])
        bar(x0, 34, int((w - x0) * .4), 6, c["track"])
        bar(x0 + int((w - x0) * .45), 34, int((w - x0) * .24), 6, c["ac"])
        bar(x0, 44, int((w - x0) * .5), 6, c["track"])
        bar(x0 + int((w - x0) * .55), 44, int((w - x0) * .2), 6, c["ok"])
        p.setClipping(False)
        p.setPen(QColor(c["line"]))
        p.drawPath(path)


class _ThemeCard(QFrame):
    def __init__(self, key: str, group: QButtonGroup):
        super().__init__()
        self.setObjectName("themeCard")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(8, 8, 8, 8)
        lay.addWidget(_ThemePreview(key))
        self.radio = QRadioButton(theme.TITLES[key])
        self.radio.setProperty("theme", key)
        group.addButton(self.radio)
        lay.addWidget(self.radio)
        self.radio.toggled.connect(self._mark)

    def _mark(self, on: bool) -> None:
        self.setProperty("selected", on)
        self.style().unpolish(self)
        self.style().polish(self)

    def mousePressEvent(self, e):
        self.radio.setChecked(True)
