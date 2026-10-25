"""Browser integration setup: native messaging host per browser and the extension folders."""

from __future__ import annotations

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import (
    QApplication,
    QDialog,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QPushButton,
    QTreeWidget,
    QTreeWidgetItem,
)

from ... import browser
from ..util import open_path
from ..widgets import CheckHeader, PillDelegate, caption, form_label, mono_font, panel, primary, shell, small_header

STEPS = {
    "chromium": "<b>Chrome, Chromium, Brave, Edge, Vivaldi, Opera:</b> open <code>chrome://extensions</code>, "
                "turn on <i>Developer mode</i>, click <i>Load unpacked</i> and choose the folder below.",
    "firefox": "<b>Firefox:</b> open <code>about:debugging#/runtime/this-firefox</code>, click "
               "<i>Load Temporary Add-on</i> and choose <code>manifest.json</code> in the folder below. "
               "Then allow it to access all websites in the add-on's permissions.",
}


class BrowserIntegrationDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Browser integration")
        self.setMinimumWidth(680)
        lay, footer = shell(self, "connection", "Browser integration",
                            "LID talks to its extension through native messaging — no network port is opened.", spacing=12)

        self.tree = QTreeWidget()
        self.header = CheckHeader(self._check_state, self.tree)
        self.header.toggled.connect(self._toggle_all)
        self.tree.setHeader(self.header)
        self.tree.setHeaderLabels(["", "BROWSER", "LID HELPER"])
        self.header.setDefaultAlignment(Qt.AlignLeft | Qt.AlignVCenter)
        self.header.setSectionResizeMode(0, QHeaderView.Fixed)
        self.header.setStretchLastSection(True)
        self.tree.setColumnWidth(0, 34)
        self.tree.setColumnWidth(1, 260)
        self.tree.setRootIsDecorated(False)
        self.tree.setAlternatingRowColors(True)
        self.tree.setItemDelegateForColumn(2, PillDelegate(
            {"installed": ("ok", "Installed"), "not installed": ("muted", "Not installed")}, self.tree))
        self.tree.itemChanged.connect(lambda *_: self.header.viewport().update())
        small_header(self.tree)
        self.tree.setMinimumHeight(28 + 5 * 27 + 2)
        lay.addWidget(self.tree, 1)
        row = QHBoxLayout()
        install = primary(QPushButton("Install for checked browsers"))
        remove = QPushButton("Remove")
        install.clicked.connect(self._install)
        remove.clicked.connect(self._remove)
        row.addWidget(install)
        row.addWidget(remove)
        row.addStretch(1)
        lay.addLayout(row)

        box, bl = panel()
        bl.addWidget(caption("Then add the LID extension to the browser"))
        self.folders = {}
        for family in ("chromium", "firefox"):
            text = QLabel(STEPS[family])
            text.setWordWrap(True)
            text.setTextFormat(Qt.RichText)
            text.setObjectName("muted")
            bl.addWidget(text)
            frow = QHBoxLayout()
            edit = QLineEdit()
            edit.setReadOnly(True)
            edit.setFont(mono_font(edit.font()))
            edit.setPlaceholderText("Install for a browser first")
            copy = QPushButton("Copy")
            copy.clicked.connect(lambda _=False, e=edit: QApplication.clipboard().setText(e.text()))
            show = QPushButton("Open folder")
            show.clicked.connect(lambda _=False, e=edit: open_path(e.text()))
            frow.addWidget(edit, 1)
            frow.addWidget(copy)
            frow.addWidget(show)
            bl.addLayout(frow)
            self.folders[family] = edit
        lay.addWidget(box)

        get = QHBoxLayout()
        get.addWidget(form_label("Get the extension:"))
        links = []
        if browser.CHROME_WEB_STORE_URL:
            links.append(("Chrome Web Store", browser.CHROME_WEB_STORE_URL))
        if browser.FIREFOX_ADDONS_URL:
            links.append(("Firefox Add-ons", browser.FIREFOX_ADDONS_URL))
        elif browser.FIREFOX_SIGNED_XPI:
            links.append(("Firefox (.xpi)", browser.FIREFOX_XPI_URL))
        else:
            links.append(("Firefox (.zip)", browser.FIREFOX_ZIP_URL))
        links.append(("All downloads", browser.RELEASES_URL))
        for text, url in links:
            b = QPushButton(text)
            b.setToolTip(url)
            b.clicked.connect(lambda _=False, u=url: QDesktopServices.openUrl(QUrl(u)))
            get.addWidget(b)
        get.addStretch(1)
        lay.addLayout(get)

        close = QPushButton("Close")
        close.setDefault(True)
        close.clicked.connect(self.reject)
        footer.addStretch(1)
        footer.addWidget(close)
        self._fill()

    def _check_state(self) -> Qt.CheckState:
        items = [self.tree.topLevelItem(i) for i in range(self.tree.topLevelItemCount())]
        states = {i.checkState(0) for i in items if not i.isDisabled()}
        if states == {Qt.Checked}:
            return Qt.Checked
        return Qt.PartiallyChecked if Qt.Checked in states else Qt.Unchecked

    def _toggle_all(self) -> None:
        on = self._check_state() != Qt.Checked
        for i in range(self.tree.topLevelItemCount()):
            item = self.tree.topLevelItem(i)
            if not item.isDisabled():
                item.setCheckState(0, Qt.Checked if on else Qt.Unchecked)

    def _fill(self):
        self.tree.clear()
        found = set(browser.detected())
        for b in browser.BROWSERS:
            installed = browser.is_installed(b)
            state = "installed" if installed else ("not installed" if b in found else "browser not found")
            item = QTreeWidgetItem(["", b.name, state])
            item.setData(0, Qt.UserRole, b)
            item.setFlags(item.flags() | Qt.ItemIsUserCheckable)
            item.setCheckState(0, Qt.Checked if b in found else Qt.Unchecked)
            if b not in found:
                item.setDisabled(True)
            self.tree.addTopLevelItem(item)

    def _checked(self) -> list[browser.Browser]:
        out = []
        for i in range(self.tree.topLevelItemCount()):
            item = self.tree.topLevelItem(i)
            if item.checkState(0) == Qt.Checked and not item.isDisabled():
                out.append(item.data(0, Qt.UserRole))
        return out

    def _install(self):
        chosen = self._checked()
        if chosen:
            browser.install(chosen)
            for family in {b.family for b in chosen}:
                self.folders[family].setText(str(browser.extension_folder(family)))
        self._fill()

    def _remove(self):
        browser.uninstall(self._checked())
        self._fill()
