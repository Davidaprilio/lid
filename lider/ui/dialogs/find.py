"""SearchBar and the search settings dialog."""

from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QCheckBox,
    QDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

from ...core.search import SearchFlags, SearchSettings, SearchSettingsError
from ..icons import bind
from ..util import separator
from ..widgets import primary


class FindDialog(QDialog):
    """"Find file" / search settings: query, targets and match options."""

    def __init__(self, settings: SearchSettings, parent=None, title: str = "Find file", show_query: bool = True):
        super().__init__(parent)
        self.settings = settings
        self.setWindowTitle(title)
        lay = QVBoxLayout(self)
        self.query = QLineEdit(settings.query)
        if show_query:
            lay.addWidget(QLabel("Find:"))
            lay.addWidget(self.query)
        self.name = QCheckBox("File name or part of the name")
        self.desc = QCheckBox("Description or part of the description")
        self.links = QCheckBox("Site name/Download link/Parent web page/Referer")
        self.case = QCheckBox("Match case")
        self.whole = QCheckBox("Match whole string only")
        f = settings.flags
        self.name.setChecked(SearchFlags.TARGET_NAME in f)
        self.desc.setChecked(SearchFlags.TARGET_DESCRIPTION in f)
        self.links.setChecked(SearchFlags.TARGET_LINKS in f)
        self.case.setChecked(SearchFlags.MATCH_CASE in f)
        self.whole.setChecked(SearchFlags.WHOLE_STRING in f)
        for c in (self.name, self.desc, self.links):
            lay.addWidget(c)
        lay.addWidget(separator())
        for c in (self.case, self.whole):
            row = QHBoxLayout()
            row.addSpacing(12)
            row.addWidget(c)
            lay.addLayout(row)
        buttons = QHBoxLayout()
        find = primary(QPushButton("Find" if show_query else "OK"))
        cancel = QPushButton("Cancel")
        find.setDefault(True)
        find.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        buttons.addStretch(1)
        buttons.addWidget(find)
        buttons.addWidget(cancel)
        buttons.addStretch(1)
        lay.addLayout(buttons)
        self.query.selectAll()

    def accept(self):
        flags = SearchFlags(0)
        for box, bit in ((self.name, SearchFlags.TARGET_NAME), (self.desc, SearchFlags.TARGET_DESCRIPTION),
                         (self.links, SearchFlags.TARGET_LINKS), (self.case, SearchFlags.MATCH_CASE),
                         (self.whole, SearchFlags.WHOLE_STRING)):
            if box.isChecked():
                flags |= bit
        trial = SearchSettings(self.query.text(), flags)
        try:
            trial.validate()
        except SearchSettingsError as e:
            # Show why, and turn "File name" back on so the dialog is usable again.
            QMessageBox.information(self, "Search settings", str(e))
            self.name.setChecked(True)
            return
        self.settings.flags = flags
        self.settings.query = self.query.text()
        super().accept()


class SearchBar(QWidget):
    """Find: [query] [Find] [Settings...] ... [x]"""

    find_requested = Signal()
    settings_requested = Signal()
    closed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(6, 2, 2, 2)
        self.edit = QLineEdit()
        self.edit.setClearButtonEnabled(True)
        self.edit.returnPressed.connect(self.find_requested)
        find = primary(QPushButton("Find"))
        find.clicked.connect(self.find_requested)
        settings = QPushButton("Settings...")
        settings.setToolTip("LID Search settings")
        settings.clicked.connect(self.settings_requested)
        close = QToolButton()
        bind(close, "close")
        close.setAutoRaise(True)
        close.setToolTip("Close SearchBar")
        close.clicked.connect(self.closed)
        lay.addWidget(QLabel("Find:"))
        lay.addWidget(self.edit, 2)
        lay.addWidget(find)
        lay.addWidget(settings)
        lay.addStretch(1)
        lay.addWidget(close, 0, Qt.AlignTop)

    def keyPressEvent(self, e):
        if e.key() == Qt.Key_Escape:
            self.closed.emit()
        else:
            super().keyPressEvent(e)
