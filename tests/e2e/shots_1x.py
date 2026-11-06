"""Screenshots of the 1.x windows (Qt, offscreen) for the side-by-side review with 2.x (tests/e2e/shots.sh).

    python3 tests/e2e/shots_1x.py OUT_DIR

Uses a copy of the 1.2 golden profile in a temporary home; file names match the 2.x ones
(<theme>--<window>.png). Windows that 1.x draws with a stock Qt dialog (font, grabber address, message
boxes) are left out.
"""

from __future__ import annotations

import os
import shutil
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(sys.argv[1]).resolve()
OUT.mkdir(parents=True, exist_ok=True)
HOME = Path(tempfile.mkdtemp(prefix="lider-shots-1x."))
shutil.copytree(ROOT / "tests/golden/v1.2/config", HOME / "config")
shutil.copytree(ROOT / "tests/golden/v1.2/data", HOME / "data")
os.environ.update(HOME=str(HOME), XDG_CONFIG_HOME=str(HOME / "config"), XDG_DATA_HOME=str(HOME / "data"),
                  XDG_CACHE_HOME=str(HOME / "cache"), QT_QPA_PLATFORM="offscreen",
                  LIDER_SOCKET_NAME=f"lider-shots-1x-{os.getpid()}")
sys.path.insert(0, str(ROOT))

from PySide6.QtWidgets import QApplication  # noqa: E402

app = QApplication(["lider"])
from lider.core import settings as settings_mod  # noqa: E402
from lider.core.manager import DownloadManager  # noqa: E402
from lider.core.store import Store  # noqa: E402
from lider.ui import theme  # noqa: E402
from lider.ui.dialogs.add import AddUrlDialog, BatchDialog, FileInfoDialog, LinksDialog  # noqa: E402
from lider.ui.dialogs.browsers import BrowserIntegrationDialog  # noqa: E402
from lider.ui.dialogs.checkhash import CheckHashDialog  # noqa: E402
from lider.ui.dialogs.find import FindDialog  # noqa: E402
from lider.ui.dialogs.options import CategoryDialog, ColumnsDialog, OptionsDialog, SpeedLimiterDialog  # noqa: E402
from lider.ui.dialogs.progress import CompleteDialog, MismatchDialog, ProgressDialog  # noqa: E402
from lider.ui.dialogs.properties import PropertiesDialog  # noqa: E402
from lider.ui.dialogs.scheduler import SchedulerDialog  # noqa: E402
from lider.ui.main_window import MainWindow  # noqa: E402

settings = settings_mod.load()
settings.proxy = ""
manager = DownloadManager(settings, Store())
win = MainWindow(manager)
LINKS = [(u, "", "a") for u in ("https://example.org/a/setup-1.2.exe", "https://example.org/a/manual.pdf",
                                "https://example.org/a/photo.jpg", "https://example.org/a/index.html")]
TABS = ["general", "file-types", "save-to", "downloads", "connection", "proxy-socks", "appearance"]


def grab(widget, name: str) -> None:
    widget.show()
    widget.adjustSize()
    for _ in range(5):
        app.processEvents()
    widget.grab().save(str(OUT / f"{name}.png"))
    widget.close()


def tabbed(make, count: int, name):
    for i in range(count):
        dlg = make()
        dlg.tabs.setCurrentIndex(i)
        grab(dlg, name(i))


for t in ("ember", "ember-dark", "dark"):
    theme.apply(t)
    win.resize(1000, 560)
    grab(win, f"{t}--main")
    win.show()
    grab(AddUrlDialog(win), f"{t}--add-address")
    grab(FileInfoDialog(settings, "http://127.0.0.1/archive.zip", "archive.zip", 65536, list(manager.queues),
                        resumable=True), f"{t}--add-file-info")
    grab(BatchDialog(win), f"{t}--batch")
    grab(LinksDialog(settings, LINKS), f"{t}--links")
    tabbed(lambda: OptionsDialog(settings, win), 7, lambda i: f"{t}--options-{i + 1}-{TABS[i]}")
    tabbed(lambda: SchedulerDialog(manager, win), 2,
           lambda i: f"{t}--scheduler-{'schedule' if i == 0 else 'files'}")
    grab(SpeedLimiterDialog(settings), f"{t}--limiter")
    grab(ColumnsDialog(settings), f"{t}--columns")
    grab(FindDialog(win.search, win), f"{t}--find")
    grab(FindDialog(win.search, win, "LID Search settings", show_query=False), f"{t}--search-settings")
    grab(CategoryDialog(None), f"{t}--category-new")
    grab(CategoryDialog(settings.category("Video")), f"{t}--category-edit")
    grab(BrowserIntegrationDialog(win), f"{t}--browsers")
    grab(ProgressDialog(manager, 2), f"{t}--progress")
    grab(CompleteDialog(manager.downloads[4]), f"{t}--complete")
    grab(MismatchDialog(manager.downloads[5]), f"{t}--mismatch")
    grab(CheckHashDialog(manager, 4), f"{t}--check-hash")
    grab(PropertiesDialog(manager, 4), f"{t}--properties")
    print(f"ok  {t}")

manager.shutdown()
shutil.rmtree(HOME, ignore_errors=True)
print(len(list(OUT.glob("*.png"))), "screenshots")
