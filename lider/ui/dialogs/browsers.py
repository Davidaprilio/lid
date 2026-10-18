"""Browser integration setup: native messaging host per browser and the extension folders."""

from __future__ import annotations

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import (
    QApplication,
    QDialog,
    QDialogButtonBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
)

from ... import browser
from ..util import dlu, open_path, page_header, separator

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
        self.resize(dlu(330, 260))
        lay = QVBoxLayout(self)
        lay.addWidget(page_header("general", "Capture downloads from the following browsers:"))
        lay.addWidget(separator())

        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Browser", "LID helper"])
        self.tree.setRootIsDecorated(False)
        self.tree.setColumnWidth(0, 220)
        self.tree.setMinimumHeight(dlu(0, 112).height())
        lay.addWidget(self.tree, 1)
        row = QHBoxLayout()
        install = QPushButton("Install for checked browsers")
        remove = QPushButton("Remove")
        install.clicked.connect(self._install)
        remove.clicked.connect(self._remove)
        row.addWidget(install)
        row.addWidget(remove)
        row.addStretch(1)
        lay.addLayout(row)
        lay.addWidget(separator())

        lay.addWidget(QLabel("Then add the LID extension to the browser:"))
        self.folders = {}
        for family in ("chromium", "firefox"):
            text = QLabel(STEPS[family])
            text.setWordWrap(True)
            text.setTextFormat(Qt.RichText)
            lay.addWidget(text)
            frow = QHBoxLayout()
            edit = QLineEdit()
            edit.setReadOnly(True)
            copy = QPushButton("Copy")
            copy.clicked.connect(lambda _=False, e=edit: QApplication.clipboard().setText(e.text()))
            show = QPushButton("Open folder")
            show.clicked.connect(lambda _=False, e=edit: open_path(e.text()))
            frow.addWidget(edit, 1)
            frow.addWidget(copy)
            frow.addWidget(show)
            lay.addLayout(frow)
            self.folders[family] = edit

        lay.addWidget(separator())
        get = QHBoxLayout()
        get.addWidget(QLabel("Get the extension:"))
        links = []
        if browser.CHROME_WEB_STORE_URL:
            links.append(("Chrome Web Store", browser.CHROME_WEB_STORE_URL))
        links.append(("Firefox Add-ons", browser.FIREFOX_ADDONS_URL) if browser.FIREFOX_ADDONS_URL
                     else ("Firefox (.xpi)", browser.FIREFOX_XPI_URL))
        links.append(("All downloads", browser.RELEASES_URL))
        for text, url in links:
            b = QPushButton(text)
            b.setToolTip(url)
            b.clicked.connect(lambda _=False, u=url: QDesktopServices.openUrl(QUrl(u)))
            get.addWidget(b)
        get.addStretch(1)
        lay.addLayout(get)

        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.rejected.connect(self.reject)
        bb.accepted.connect(self.accept)
        lay.addWidget(bb)
        self._fill()

    def _fill(self):
        self.tree.clear()
        found = set(browser.detected())
        for b in browser.BROWSERS:
            installed = browser.is_installed(b)
            state = "installed" if installed else ("not installed" if b in found else "browser not found")
            item = QTreeWidgetItem([b.name, state])
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
