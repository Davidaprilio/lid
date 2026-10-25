"""The one-time offer of an AppImage to add LID to the application menu."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtWidgets import QDialog, QPushButton, QWidget

from ... import desktop
from ...core import settings as settings_mod
from ...core.settings import Settings
from ..widgets import note, primary, shell


class AddToMenuDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("LID")
        self.setMinimumWidth(520)
        name = Path(desktop.appimage()).name or "LID"
        body, footer = shell(self, "general", "Add LID to the application menu?",
                             f"{name} runs on its own, so the application menu does not list LID yet.",
                             badge="accent")
        body.addWidget(note("LID keeps the entry pointed at the AppImage you start, also after you move or "
                            "update it, and it disappears when the AppImage is deleted. You can change this "
                            "later in Options → General.", boxed=True))
        later = QPushButton("Not now")
        add = primary(QPushButton("Add to menu"))
        later.clicked.connect(self.reject)
        add.clicked.connect(self.accept)
        footer.addStretch(1)
        footer.addWidget(later)
        footer.addWidget(add)
        add.setDefault(True)


def should_offer(settings: Settings) -> bool:
    return bool(desktop.appimage()) and not (settings.menu_offered or desktop.in_menu()
                                             or desktop.system_installed() or desktop.listed_elsewhere())


def offer(parent: QWidget, settings: Settings) -> None:
    """Ask once; the answer is remembered either way."""
    settings.menu_offered = True
    settings_mod.save(settings)
    if AddToMenuDialog(parent).exec() == QDialog.Accepted:
        try:
            desktop.add_to_menu()
        except OSError:
            pass
