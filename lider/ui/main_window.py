"""Main window: Categories tree on the left, download list on the right, menus and toolbar on top."""

from __future__ import annotations

import os
import subprocess
from functools import partial
from urllib.parse import urlsplit

from PySide6.QtCore import QByteArray, QItemSelectionModel, QPoint, QSize, Qt, QTimer
from PySide6.QtGui import QAction, QActionGroup, QFont, QGuiApplication, QKeySequence
from PySide6.QtWidgets import (
    QAbstractItemView,
    QApplication,
    QCheckBox,
    QFileDialog,
    QFontDialog,
    QHBoxLayout,
    QHeaderView,
    QInputDialog,
    QLabel,
    QMainWindow,
    QMenu,
    QMenuBar,
    QMessageBox,
    QSizePolicy,
    QSplitter,
    QStatusBar,
    QSystemTrayIcon,
    QToolBar,
    QToolButton,
    QTreeView,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from .. import APP_LONG_NAME, APP_NAME, PACKAGE_NAME, __version__
from ..core import categories, media, settings as settings_mod
from ..core.engine import filename_from_url, safe_filename
from ..core.manager import DownloadManager, Event
from ..core.models import MAIN_QUEUE, SYNC_QUEUE, Download, Status
from ..core.search import FindNext, SearchFlags, SearchSettings, SearchSettingsError, filter_downloads
from .dialogs.add import (
    URL_RE,
    AddUrlDialog,
    BatchDialog,
    DuplicateDialog,
    FileInfoDialog,
    LinksDialog,
    extract_links,
)
from .dialogs.find import FindDialog, SearchBar
from .dialogs.options import CategoryDialog, ColumnsDialog, OptionsDialog, SpeedLimiterDialog
from .dialogs.progress import CompleteDialog, ProgressDialog
from .dialogs.properties import PropertiesDialog
from .dialogs.scheduler import SchedulerDialog
from .details import DetailsPane
from .download_model import CHECK, ID_ROLE, SORT_KEYS, DownloadModel
from . import theme
from .icons import bind, category_icon, icon
from .util import open_folder, open_path, run_async
from .widgets import COUNT_ROLE, CheckHeader, ListDelegate, TreeDelegate, mono_font

ARRANGE = [
    ("By &Order Of Addition", "date_added"),
    ("By &File Name", "filename"),
    ("By &Size", "size"),
    ("By St&atus", "status"),
    ("By &Time Left", "time_left"),
    ("By Transfer &Rate", "rate"),
    ("By &Last Try Date", "last_try"),
    ("By &Description", "description"),
    ("By Save &Path", "save_to"),
    ("By R&eferer", "referer"),
    ("By Parent &Web Page", "parent_page"),
]
HIDDEN_TREE_CATEGORIES = {"General"}  # General files are listed only under "All Downloads"
ROW_HEIGHT = {"comfortable": 32, "compact": 26}
MIN_WIDTH = {"queue": 34, "size": 88, "status": 170, "time_left": 96, "rate": 116}  # pills and mono numbers need room


class MainWindow(QMainWindow):
    def __init__(self, manager: DownloadManager):
        super().__init__()
        self.manager = manager
        self.settings = manager.settings
        self.progress_dialogs: dict[int, ProgressDialog] = {}
        self.search = SearchSettings(self.settings.search_string, SearchFlags(self.settings.search_flags & 0x1F))
        self.finder = FindNext(self.search)
        self.tree_filter: tuple[str, str | None] = ("all", None)
        self.really_quit = False
        self._last_clip = ""

        self.setWindowTitle(f"{APP_LONG_NAME} {__version__}")
        self.setWindowIcon(icon("app"))
        self.setAcceptDrops(True)
        self.resize(1000, 560)

        self._build_actions()
        self._build_menus()
        self._build_toolbar()
        self._build_central()
        self._build_statusbar()
        self._build_tray()

        if self.settings.window_geometry:
            self.restoreGeometry(QByteArray.fromBase64(self.settings.window_geometry.encode()))
        if self.settings.list_font:
            f = QFont()
            if f.fromString(self.settings.list_font):
                self.view.setFont(f)
        self.apply_theme()
        hints = QGuiApplication.styleHints()
        if hasattr(hints, "colorSchemeChanged"):
            hints.colorSchemeChanged.connect(lambda *_: self.apply_theme())
        if self.settings.speed_limiter_on_startup:
            self.manager.set_speed_limiter(True)
        self._sync_limiter_actions()

        QApplication.clipboard().dataChanged.connect(self._clipboard_changed)
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.tick)
        self.timer.start(500)
        self.tick()

    # ------------------------------------------------------------------ actions
    def _act(self, text, slot, shortcut=None, icon_name=None, tip=None, checkable=False) -> QAction:
        a = QAction(text, self)
        if icon_name:
            bind(a, icon_name)
        if shortcut:
            a.setShortcut(QKeySequence(shortcut))
        if tip:
            a.setToolTip(tip)
            a.setStatusTip(tip)
        a.setCheckable(checkable)
        a.triggered.connect(slot)
        return a

    def _build_actions(self):
        A = self._act
        self.a_add = A("&Add new download", lambda: self.add_url(), "Ctrl+N", "add-url",
                       "Add new download address manually")
        self.a_batch = A("Add &batch download", self.batch_download)
        self.a_batch_clip = A("Add batch download from &clipboard", self.batch_from_clipboard)
        self.a_grabber = A("Run site &grabber", self.run_grabber, None, "grabber", "Run site grabber")
        self.a_export_ef2 = A("To .ef2 export &file", self.export_ef2)
        self.a_export_txt = A("To &text file", self.export_text)
        self.a_import_ef2 = A("&From .ef2 export file", self.import_ef2)
        self.a_import_txt = A("From &text file", self.import_text)
        self.a_exit = A("&Exit", self.quit_app, "Ctrl+Q")

        self.a_stop = A("&Stop Download", self.stop_selected, None, "stop", "Stop the checked (or selected) downloads")
        self.a_remove = A("&Remove", self.remove_selected, "Del", "delete", "Remove the checked (or selected) files from the list")
        self.a_resume = A("&Download Now", self.resume_selected, None, "resume",
                          "Start or resume the checked (or selected) downloads")
        self.a_redownload = A("R&edownload", self.redownload_selected)

        self.a_pause_all = A("&Pause All", self.manager.stop_all)
        self.a_stop_all = A("&Stop All", self.manager.stop_all, None, "stop-all", "Stop all downloads")
        self.a_delete_completed = A("&Delete All Completed", self.delete_completed, None, "delete-completed",
                                    "Remove all completed files from the list")
        self.a_find = A("Find (Ctrl-F)", self.show_searchbar, "Ctrl+F", "search")
        self.a_find_next = A("Find Next (F3)", self.find_next, "F3")
        self.a_scheduler = A("S&cheduler", lambda: self.open_scheduler(), None, "scheduler",
                             "Scheduler and download queues")
        self.a_limit_on = A("Turn o&n", lambda: self.set_limiter(True), checkable=True)
        self.a_limit_off = A("Turn of&f", lambda: self.set_limiter(False), checkable=True)
        grp = QActionGroup(self)
        grp.addAction(self.a_limit_on)
        grp.addAction(self.a_limit_off)
        self.a_limit_settings = A("S&ettings...", self.limiter_settings)
        self.a_options = A("&Options", self.open_options, None, "options",
                           "Integration, file types, save folders, connections, proxy and others")

        self.a_hide_cats = A("Hide &categories", self.toggle_categories, checkable=True)
        self.a_hide_cats.setChecked(not self.settings.show_categories)
        self.a_details = A("&Details pane", lambda on: self.set_details_visible(on), "F9", checkable=True)
        self.a_details.setChecked(self.settings.show_details)
        self.a_statusbar = A("Status &bar", lambda on: self.set_statusbar_visible(on), checkable=True)
        self.a_statusbar.setChecked(self.settings.show_statusbar)
        self.menubar_group = QActionGroup(self)
        self.menubar_actions = {}
        for key, text in (("native", "&Native (desktop menu)"), ("custom", "In the &window"), ("both", "&Both")):
            a = A(text, partial(self.set_menubar_mode, key), checkable=True)
            a.setChecked(key == self.settings.menubar)
            self.menubar_group.addAction(a)
            self.menubar_actions[key] = a
        self.a_labels = A("Icons and &labels", lambda: self.set_toolbar_labels(True), checkable=True)
        self.a_icons_only = A("&Icons only", lambda: self.set_toolbar_labels(False), checkable=True)
        g2 = QActionGroup(self)
        g2.addAction(self.a_labels)
        g2.addAction(self.a_icons_only)
        (self.a_labels if self.settings.toolbar_labels else self.a_icons_only).setChecked(True)
        self.a_columns = A("C&ustomize URL List...", self.customize_columns)
        self.theme_group = QActionGroup(self)
        self.theme_actions = {}
        for key in ("system", *theme.THEMES):
            text = "Follow the system" if key == "system" else theme.TITLES[key]
            a = A(text, partial(self.choose_theme, key), checkable=True)
            self.theme_group.addAction(a)
            self.theme_actions[key] = a
        self._sync_theme_actions()
        self.a_font = A("Choose Font...", self.choose_font)
        self.a_font_reset = A("Reset to default Font", self.reset_font)
        self.a_about = A("&About LID", self.about)

        self.arrange_group = QActionGroup(self)
        self.arrange_actions = []
        for text, key in ARRANGE:
            a = A(text, partial(self.arrange, key), checkable=True)
            a.setChecked(key == self.settings.sort_key)
            self.arrange_group.addAction(a)
            self.arrange_actions.append(a)

    def _build_menus(self):
        mb = self.menuBar()
        tasks = mb.addMenu("&Tasks")
        tasks.addActions([self.a_add, self.a_batch, self.a_batch_clip, self.a_grabber])
        tasks.addSeparator()
        exp = tasks.addMenu("E&xport")
        exp.addActions([self.a_export_ef2, self.a_export_txt])
        imp = tasks.addMenu("&Import")
        imp.addActions([self.a_import_ef2, self.a_import_txt])
        tasks.addSeparator()
        tasks.addAction(self.a_exit)

        file = mb.addMenu("&File")
        file.addActions([self.a_stop, self.a_remove, self.a_resume, self.a_redownload])

        dl = mb.addMenu("&Downloads")
        dl.addActions([self.a_pause_all, self.a_stop_all])
        dl.addSeparator()
        dl.addAction(self.a_delete_completed)
        dl.addSeparator()
        dl.addActions([self.a_find, self.a_find_next])
        dl.addSeparator()
        dl.addAction(self.a_scheduler)
        self.start_queue_menu = dl.addMenu("Start &queue")
        self.stop_queue_menu = dl.addMenu("S&top queue")
        self.start_queue_menu.aboutToShow.connect(lambda: self._fill_queue_menu(self.start_queue_menu, True))
        self.stop_queue_menu.aboutToShow.connect(lambda: self._fill_queue_menu(self.stop_queue_menu, False))
        dl.addSeparator()
        lim = dl.addMenu("Speed &Limiter")
        lim.addActions([self.a_limit_on, self.a_limit_off])
        lim.addSeparator()
        lim.addAction(self.a_limit_settings)
        dl.addSeparator()
        dl.addAction(self.a_options)

        view = mb.addMenu("&View")
        view.addActions([self.a_hide_cats, self.a_details, self.a_statusbar])
        view.addSeparator()
        arrange = view.addMenu("&Arrange files")
        arrange.addActions(self.arrange_actions)
        tb = view.addMenu("&Toolbar")
        tb.addActions([self.a_labels, self.a_icons_only])
        mbm = view.addMenu("&Menu bar")
        mbm.addActions(list(self.menubar_actions.values()))
        view.addAction(self.a_columns)
        themes = view.addMenu("T&heme")
        themes.addAction(self.theme_actions["system"])
        themes.addSeparator()
        themes.addActions([self.theme_actions[k] for k in theme.THEMES])
        font = view.addMenu("Font")
        font.addActions([self.a_font, self.a_font_reset])

        help_menu = mb.addMenu("&Help")
        help_menu.addAction(self.a_about)

        # The in-window copy used by "custom" and "both": the same menus in a plain QMenuBar.
        self.inline_menubar = QMenuBar()
        self.inline_menubar.setNativeMenuBar(False)
        for menu in (tasks, file, dl, view, help_menu):
            self.inline_menubar.addMenu(menu)

    def _tool(self, action: QAction | None, text: str, name: str, icon_name: str | None = None) -> QToolButton:
        b = QToolButton()
        if action is not None:
            action.setIconText(text)  # toolbar caption; the menus keep the long text
            b.setDefaultAction(action)
        b.setText(text)
        b.setObjectName(name)
        if icon_name:
            bind(b, icon_name)
        b.setToolButtonStyle(Qt.ToolButtonTextBesideIcon)
        b.setIconSize(QSize(18, 18))
        b.setCursor(Qt.PointingHandCursor)
        return b

    def _build_toolbar(self):
        self.menu_toolbar = QToolBar("Menu bar")
        self.menu_toolbar.setObjectName("menubar")
        self.menu_toolbar.setMovable(False)
        self.menu_toolbar.toggleViewAction().setVisible(False)
        self._menubar_action = self.menu_toolbar.addWidget(self.inline_menubar)
        self.addToolBar(self.menu_toolbar)
        self.addToolBarBreak()
        self.toolbar = tb = QToolBar("Toolbar")
        tb.setObjectName("toolbar")
        tb.setMovable(False)
        self.addToolBar(tb)
        add = self._tool(self.a_add, "Add URL", "primary", "add-white")
        start = self._tool(self.a_resume, "Start", "pill")
        stop = self._tool(self.a_stop, "Stop", "pill")
        delete = self._tool(self.a_remove, "Delete", "danger")
        menu = QMenu(delete)
        menu.addAction(icon("delete"), "Delete checked or selected", self.remove_selected)
        menu.addAction(icon("delete-completed"), "Delete all completed", self.delete_completed)
        delete.setMenu(menu)
        delete.setPopupMode(QToolButton.MenuButtonPopup)
        self.check_chip = QToolButton()
        self.check_chip.setObjectName("chip")
        self.check_chip.setToolButtonStyle(Qt.ToolButtonTextBesideIcon)
        self.check_chip.setLayoutDirection(Qt.RightToLeft)
        bind(self.check_chip, "close")
        self.check_chip.setIconSize(QSize(11, 11))
        self.check_chip.setToolTip("Clear the checks")
        self.check_chip.clicked.connect(lambda: self.model.set_all_checked(False))
        spacer = QWidget()
        spacer.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        options = self._tool(self.a_options, "Options", "ghost")
        sched = self._tool(self.a_scheduler, "Scheduler", "ghost")
        grab = self._tool(self.a_grabber, "Grabber", "ghost")
        tb.addWidget(add)
        tb.addSeparator()
        tb.addWidget(start)
        tb.addWidget(stop)
        tb.addSeparator()
        tb.addWidget(delete)
        tb.addSeparator()
        self._chip_action = tb.addWidget(self.check_chip)
        tb.addWidget(spacer)
        for b in (options, sched, grab):
            tb.addWidget(b)
        self._toolbar_buttons = [add, start, stop, delete, options, sched, grab]
        self.set_toolbar_labels(self.settings.toolbar_labels)
        self.set_menubar_mode(self.settings.menubar)

    def _build_central(self):
        # -- left: "Categories" group with a close button
        self.cat_panel = QWidget()
        cl = QVBoxLayout(self.cat_panel)
        cl.setContentsMargins(0, 4, 0, 4)
        head = QHBoxLayout()
        head.setContentsMargins(10, 6, 4, 2)
        title = QLabel("CATEGORIES")
        title.setObjectName("sectionTitle")
        head.addWidget(title)
        head.addStretch(1)
        close = QToolButton()
        close.setObjectName("close")
        bind(close, "close")
        close.setAutoRaise(True)
        close.setToolTip("Hide categories")
        close.clicked.connect(lambda: self.toggle_categories(True))
        head.addWidget(close)
        cl.addLayout(head)
        self.tree = QTreeWidget()
        self.tree.setObjectName("categories")
        self.tree.setItemDelegate(TreeDelegate(self.tree))
        self.tree.setIndentation(16)
        self.tree.setHeaderHidden(True)
        self.tree.setContextMenuPolicy(Qt.CustomContextMenu)
        self.tree.customContextMenuRequested.connect(self._tree_menu)
        self.tree.currentItemChanged.connect(self._tree_changed)
        cl.addWidget(self.tree)

        # -- right: SearchBar + download list
        right = QWidget()
        rl = QVBoxLayout(right)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(0)
        self.searchbar = SearchBar()
        self.searchbar.setVisible(False)
        self.searchbar.find_requested.connect(self.apply_search)
        self.searchbar.settings_requested.connect(self.search_settings)
        self.searchbar.closed.connect(self.close_searchbar)
        rl.addWidget(self.searchbar)
        self.model = DownloadModel(self.manager, self)
        self.model.checked_changed.connect(self.update_actions)
        self.view = QTreeView()
        self.view.setObjectName("downloads")
        self.header = CheckHeader(self.model.check_state, self.view)
        self.header.setDefaultAlignment(Qt.AlignLeft | Qt.AlignVCenter)
        self.header.toggled.connect(self.model.toggle_all)
        self.view.setHeader(self.header)
        self.view.setModel(self.model)
        self.list_delegate = ListDelegate(self.model, self.view)
        self.view.setItemDelegate(self.list_delegate)
        self.view.setAlternatingRowColors(True)
        self.view.setRootIsDecorated(False)
        self.view.setUniformRowHeights(True)
        self.view.setAllColumnsShowFocus(True)
        self.view.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.view.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.view.setDragEnabled(False)
        self.view.setContextMenuPolicy(Qt.CustomContextMenu)
        self.view.customContextMenuRequested.connect(self._list_menu)
        self.view.doubleClicked.connect(self._double_clicked)
        header = self.view.header()
        header.setSectionsMovable(False)
        header.setStretchLastSection(True)
        header.setSectionsClickable(True)
        header.sectionClicked.connect(self._header_clicked)
        header.sectionResized.connect(self._column_resized)
        header.setContextMenuPolicy(Qt.CustomContextMenu)
        header.customContextMenuRequested.connect(lambda _p: self.customize_columns())
        self.view.selectionModel().selectionChanged.connect(lambda *_: self.update_actions())
        self.view.selectionModel().currentChanged.connect(lambda *_: self._show_details())
        self.apply_column_widths()

        self.details = DetailsPane(self.manager)
        self.details.closed.connect(lambda: self.set_details_visible(False))
        self.vsplit = QSplitter(Qt.Vertical)
        self.vsplit.addWidget(self.view)
        self.vsplit.addWidget(self.details)
        self.vsplit.setStretchFactor(0, 1)
        self.vsplit.setSizes([420, 210])
        self.vsplit.setChildrenCollapsible(False)
        rl.addWidget(self.vsplit)
        self.details.setVisible(self.settings.show_details)

        self.splitter = QSplitter()
        self.splitter.addWidget(self.cat_panel)
        self.splitter.addWidget(right)
        self.splitter.setStretchFactor(1, 1)
        self.splitter.setSizes([236, 800])
        self.setCentralWidget(self.splitter)
        self.cat_panel.setVisible(self.settings.show_categories)
        self.rebuild_tree()

    def _build_statusbar(self):
        sb = QStatusBar()
        sb.setSizeGripEnabled(False)
        self.setStatusBar(sb)
        mono = mono_font()
        self.sb_speed = QLabel()
        self.sb_speed.setFont(mono)
        self.sb_counts = QLabel()
        self.sb_limiter, self.sb_clip, self.sb_browser = QLabel(), QLabel(), QLabel()
        sb.addWidget(self.sb_speed)
        sb.addWidget(self.sb_counts)
        for w in (self.sb_limiter, self.sb_clip, self.sb_browser):
            sb.addPermanentWidget(w)
        sb.setVisible(self.settings.show_statusbar)

    def update_statusbar(self):
        if not self.statusBar().isVisible():
            return
        from ..core import fmt
        from .. import browser

        rate = sum(t.meter.rate() for t in self.manager.tasks.values() if getattr(t, "meter", None))
        self.sb_speed.setText(f"↓ {fmt.rate(rate) or '0 B/s'}")
        queued = sum(1 for d in self.manager.downloads.values() if d.queue and d.status != Status.COMPLETE
                     and not self.manager.is_running(d.id))
        self.sb_counts.setText(f"{len(self.manager.tasks)} downloading · {queued} in queue")
        self.sb_limiter.setText(f"Speed Limiter: {'on' if self.settings.speed_limiter_on else 'off'}")
        self.sb_clip.setText(f"Clipboard: {'on' if self.settings.monitor_clipboard else 'off'}")
        if not hasattr(self, "_browser_ok"):
            try:
                self._browser_ok = any(browser.is_installed(b) for b in browser.detected())
            except Exception:  # never let a status line break the window
                self._browser_ok = False
        self.sb_browser.setText(f"Browser: {'connected' if self._browser_ok else 'not set up'}")

    def _build_tray(self):
        self.tray = None
        if not self.settings.tray_icon or not QSystemTrayIcon.isSystemTrayAvailable():
            return
        self.tray = QSystemTrayIcon(icon("app"), self)
        menu = QMenu()
        menu.addAction("Open LID", self.show_normal)
        menu.addAction(self.a_add)
        menu.addSeparator()
        lim = menu.addMenu("Speed Limiter")
        lim.addActions([self.a_limit_on, self.a_limit_off])
        menu.addSeparator()
        menu.addAction(self.a_exit)
        self.tray.setContextMenu(menu)
        self.tray.activated.connect(
            lambda reason: self.show_normal() if reason == QSystemTrayIcon.Trigger else None)
        self.tray.show()

    # ------------------------------------------------------------------ tree
    def rebuild_tree(self):
        current = self.tree_filter
        self.tree.blockSignals(True)
        self.tree.clear()

        def node(parent, text, data, icon_name):
            item = QTreeWidgetItem(parent, [text])
            item.setData(0, Qt.UserRole, data)
            item.setIcon(0, icon(icon_name))
            return item


        cats = [c.name for c in self.settings.categories if c.name not in HIDDEN_TREE_CATEGORIES]
        for kind, title, ic in (("all", "All Downloads", "folder-download"), ("unfinished", "Unfinished", "unfinished"),
                                ("finished", "Finished", "finished")):
            top = node(self.tree, title, (kind, None), ic)
            for c in cats:
                node(top, c, (kind, c), category_icon(c))
            top.setExpanded(kind == "all")
        node(self.tree, "Grabber projects", ("grabber", None), "grabber")
        queues = node(self.tree, "Queues", ("queues", None), "queue")
        for q in [MAIN_QUEUE, SYNC_QUEUE] + sorted(n for n in self.manager.queues if n not in (MAIN_QUEUE, SYNC_QUEUE)):
            node(queues, q, ("queue", q), "queue-sync" if q == SYNC_QUEUE else "queue")
        queues.setExpanded(True)
        self.tree.blockSignals(False)
        self._select_tree(current)
        self.update_tree_counts()

    def update_tree_counts(self):
        downloads = list(self.manager.downloads.values())
        for item in _iter_items(self.tree):
            kind, value = tuple(item.data(0, Qt.UserRole))
            if kind in ("all", "unfinished", "finished"):
                n = sum(1 for d in downloads if (kind == "all" or (d.status == Status.COMPLETE) == (kind == "finished"))
                        and (value is None or d.category == value))
            elif kind == "queue":
                n = sum(1 for d in downloads if d.queue == value and d.status != Status.COMPLETE)
            else:
                n = 0
            if item.data(0, COUNT_ROLE) != n:
                item.setData(0, COUNT_ROLE, n)

    def _select_tree(self, data):
        it = _iter_items(self.tree)
        for item in it:
            if tuple(item.data(0, Qt.UserRole)) == tuple(data):
                self.tree.setCurrentItem(item)
                return
        self.tree.setCurrentItem(self.tree.topLevelItem(0))

    def _tree_changed(self, item, _prev):
        if item is not None:
            self.tree_filter = tuple(item.data(0, Qt.UserRole))
            self.finder.reset()
            self.refresh_rows()

    def _tree_menu(self, pos: QPoint):
        item = self.tree.itemAt(pos)
        kind, value = tuple(item.data(0, Qt.UserRole)) if item else ("", None)
        menu = QMenu(self)
        if kind in ("queue", "queues"):
            if kind == "queue":
                menu.addAction("Start", lambda: self.start_queue(value))
                menu.addAction("Stop", lambda: self.stop_queue(value))
                menu.addSeparator()
            menu.addAction("Scheduler...", lambda: self.open_scheduler(value or MAIN_QUEUE))
        else:
            menu.addAction("Add category", self.add_category)
            act_del = menu.addAction("Delete", lambda: self.delete_category(value))
            act_prop = menu.addAction("Properties", lambda: self.edit_category(value))
            act_del.setEnabled(bool(value))
            act_prop.setEnabled(bool(value))
        menu.exec(self.tree.viewport().mapToGlobal(pos))

    def add_category(self, parent=None) -> str | None:
        dlg = CategoryDialog(None, parent or self)
        if dlg.exec():
            cat = dlg.category()
            if all(c.name != cat.name for c in self.settings.categories):
                self.settings.categories.append(cat)
                self.save_settings()
                self.rebuild_tree()
                return cat.name
        return None

    def edit_category(self, name):
        cat = self.settings.category(name)
        dlg = CategoryDialog(cat, self)
        if dlg.exec():
            new = dlg.category()
            idx = self.settings.categories.index(cat)
            self.settings.categories[idx] = new
            if new.name != name:
                for d in self.manager.downloads.values():
                    if d.category == name:
                        d.category = new.name
                        self.manager.update(d)
            self.save_settings()
            self.rebuild_tree()

    def delete_category(self, name):
        if QMessageBox.question(self, "Deleting a category from LID categories list",
                                f'Do you really want to delete "{name}" category from LID categories list?') \
                != QMessageBox.Yes:
            return
        self.settings.categories = [c for c in self.settings.categories if c.name != name]
        for d in self.manager.downloads.values():
            if d.category == name:
                d.category = "General"
                self.manager.update(d)
        self.save_settings()
        self.tree_filter = ("all", None)
        self.rebuild_tree()

    # ------------------------------------------------------------------ list
    def visible_downloads(self) -> list[Download]:
        kind, value = self.tree_filter
        items = list(self.manager.downloads.values())
        if kind == "unfinished":
            items = [d for d in items if d.status != Status.COMPLETE]
        elif kind == "finished":
            items = [d for d in items if d.status == Status.COMPLETE]
        elif kind == "queue":
            items = [d for d in items if d.queue == value]
        elif kind in ("grabber", "queues"):
            items = [d for d in items if d.queue] if kind == "queues" else []
        if value and kind in ("all", "unfinished", "finished"):
            items = [d for d in items if d.category == value]
        try:
            items = filter_downloads(items, self.search)
        except SearchSettingsError:
            pass
        if kind == "queue":
            return sorted(items, key=lambda d: d.queue_order)
        key = SORT_KEYS.get(self.settings.sort_key, SORT_KEYS["date_added"])
        return sorted(items, key=key, reverse=self.settings.sort_desc)

    def selected_ids(self) -> list[int]:
        rows = sorted({i.row() for i in self.view.selectionModel().selectedRows()})
        return [self.model.rows[r].id for r in rows if r < len(self.model.rows)]

    def checked_ids(self) -> list[int]:
        return [d.id for d in self.model.rows if d.id in self.model.checked]

    def target_ids(self) -> list[int]:
        """What Start, Stop and Delete act on: the checked rows, or the selection when nothing is checked."""
        return self.checked_ids() or self.selected_ids()

    def refresh_rows(self):
        selected = set(self.selected_ids())
        current = self.view.currentIndex()
        current_id = self.model.rows[current.row()].id if current.isValid() and current.row() < len(self.model.rows) else None
        before = [d.id for d in self.model.rows]
        self.model.set_rows(self.visible_downloads())
        if [d.id for d in self.model.rows] != before:
            sm = self.view.selectionModel()
            for r, d in enumerate(self.model.rows):
                if d.id in selected:
                    sm.select(self.model.index(r, 0), QItemSelectionModel.Select | QItemSelectionModel.Rows)
                if d.id == current_id:
                    sm.setCurrentIndex(self.model.index(r, 0), QItemSelectionModel.NoUpdate)
            self.apply_column_widths()
        self.update_actions()

    def apply_column_widths(self):
        widths = {k: w for k, v, w in self.settings.columns if v}
        widths[CHECK] = 40
        self.view.header().blockSignals(True)
        for i, key in enumerate(self.model.columns):
            self.view.setColumnWidth(i, max(widths.get(key, 100), MIN_WIDTH.get(key, 0)))
        self.view.header().setSectionResizeMode(0, QHeaderView.Fixed)
        self.view.header().blockSignals(False)

    def _column_resized(self, index, _old, new):
        if index < len(self.model.columns):
            key = self.model.columns[index]
            for c in self.settings.columns:
                if c[0] == key:
                    c[2] = new

    def _header_clicked(self, index):
        key = self.model.columns[index]
        if key == CHECK:
            return
        if key not in SORT_KEYS:
            return
        if self.settings.sort_key == key:
            self.settings.sort_desc = not self.settings.sort_desc
        else:
            self.settings.sort_key, self.settings.sort_desc = key, False
        self.view.header().setSortIndicatorShown(True)
        self.view.header().setSortIndicator(index, Qt.DescendingOrder if self.settings.sort_desc else Qt.AscendingOrder)
        for a, (_t, k) in zip(self.arrange_actions, ARRANGE):
            a.setChecked(k == key)
        self.refresh_rows()

    def arrange(self, key: str):
        self.settings.sort_key, self.settings.sort_desc = key, False
        self.refresh_rows()

    def update_actions(self):
        ids = self.target_ids()
        ds = [self.manager.downloads[i] for i in ids if i in self.manager.downloads]
        n = len(self.checked_ids())
        self.check_chip.setText(f"{n} checked")
        self._chip_action.setVisible(n > 0)
        self.header.viewport().update()
        running = [d for d in ds if self.manager.is_running(d.id)]
        resumable = [d for d in ds if not self.manager.is_running(d.id) and d.status != Status.COMPLETE]
        self.a_resume.setEnabled(bool(resumable))
        self.a_stop.setEnabled(bool(running))
        self.a_remove.setEnabled(bool(ds))
        self.a_redownload.setEnabled(bool(ds) and not running)
        self.a_stop_all.setEnabled(bool(self.manager.tasks))
        self.a_delete_completed.setEnabled(any(d.status == Status.COMPLETE for d in self.manager.downloads.values()))

    def _list_menu(self, pos):
        ids = self.selected_ids()
        if not ids:
            menu = QMenu(self)
            menu.addAction(self.a_add)
            menu.exec(self.view.viewport().mapToGlobal(pos))
            return
        d = self.manager.downloads[ids[0]]
        complete = d.status == Status.COMPLETE
        menu = QMenu(self)
        a = menu.addAction("Open", lambda: open_path(d.path))
        a.setEnabled(complete)
        a = menu.addAction("Open with...", lambda: self.open_with(d))
        a.setEnabled(complete)
        menu.addAction("Open folder", lambda: open_folder(d.path) if complete else open_path(d.save_dir))
        menu.addSeparator()
        menu.addAction("Move/Rename", lambda: self.properties(d.id))
        menu.addAction(self.a_redownload)
        menu.addSeparator()
        menu.addAction(self.a_resume)
        menu.addAction(self.a_stop)
        menu.addAction(self.a_remove)
        menu.addSeparator()
        add_q = menu.addMenu("Add to queue")
        for q in self._queue_names():
            add_q.addAction(q, partial(self.move_to_queue, ids, q))
        a = menu.addAction("Delete from queue", lambda: self.move_to_queue(ids, ""))
        a.setEnabled(any(self.manager.downloads[i].queue for i in ids))
        menu.addSeparator()
        menu.addAction("Copy download address", lambda: QApplication.clipboard().setText(d.url))
        menu.addAction("Properties", lambda: self.properties(d.id))
        menu.exec(self.view.viewport().mapToGlobal(pos))

    def _double_clicked(self, index):
        i = index.data(ID_ROLE) if index.isValid() else None
        if i is None:
            i = self.model.rows[index.row()].id
        d = self.manager.downloads[i]
        if d.status == Status.COMPLETE:
            self.properties(i)
        else:
            if not self.manager.is_running(i):
                self.manager.start(i)
            self.show_progress(i)

    def move_to_queue(self, ids, name):
        self.manager.set_queue(ids, name)
        self.refresh_rows()

    # ------------------------------------------------------------------ add downloads
    def add_url(self, url: str | None = None, referer: str = "", description: str = "",
                parent_page: str = "", ask_address: bool = True, headers: dict | None = None,
                filename: str = ""):
        username = password = ""
        if url is None or (ask_address and not url):
            dlg = AddUrlDialog(self, url or "")
            if not dlg.exec():
                return
            url = dlg.url()
            username, password = dlg.auth.credentials()

        dup = self.manager.find_duplicate(url)
        overwrite = False
        if dup:
            action = self.settings.duplicate_action
            if action == "ask":
                dd = DuplicateDialog(url, self)
                if not dd.exec():
                    return
                action = dd.choice()
                if dd.remember.isChecked():
                    self.settings.duplicate_action = action
                    self.save_settings()
            if action == "complete":
                if dup.status == Status.COMPLETE:
                    CompleteDialog(dup, self).show()
                else:
                    self.manager.start(dup.id)
                    self.show_progress(dup.id)
                return
            overwrite = action == "overwrite"

        if media.is_youtube(url):
            self._add_youtube(url, dup if overwrite else None)
            return

        QApplication.setOverrideCursor(Qt.BusyCursor)

        def probe():
            return self.manager.probe(url, referer, username, password, headers)

        def done(result, error):
            QApplication.restoreOverrideCursor()
            if error is not None:
                ans = QMessageBox.question(
                    self, "LID", f"Cannot get information about the file:\n{error}\n\nAdd it anyway?")
                if ans != QMessageBox.Yes:
                    return
                name, size, resumable = safe_filename(filename or filename_from_url(url)), -1, None
            else:
                name, size, resumable = result.filename, result.size, result.resumable
                if filename and "." in filename and "." not in name:
                    name = safe_filename(filename)  # the browser already knew a better name
            self._confirm_and_add(url, name, size, resumable, referer, description, parent_page,
                                  username, password, dup if overwrite else None, headers)

        run_async(probe, done)

    def _add_youtube(self, url: str, replace: Download | None):
        if not media.available():
            QMessageBox.warning(self, "LID", "Downloading from YouTube needs the yt-dlp package:\n\n"
                                "    pip install yt-dlp")
            return
        QApplication.setOverrideCursor(Qt.BusyCursor)

        def done(info, error):
            QApplication.restoreOverrideCursor()
            if error is not None:
                QMessageBox.warning(self, "LID", f"Cannot read this YouTube video:\n{str(error).replace('ERROR: ', '')}")
                return
            if not info.variants:
                QMessageBox.warning(self, "LID", "This YouTube video has no downloadable formats.")
                return
            self._confirm_youtube(info, replace)

        run_async(lambda: media.probe(url), done)

    def _confirm_youtube(self, info: media.MediaInfo, replace: Download | None):
        s = self.settings
        v = info.default_variant(s.media_kind, s.media_quality)
        name = media.suggested_filename(info, v)
        dlg = FileInfoDialog(s, info.url, name, v.size, self._queue_names(), "", self,
                             new_category=self.add_category, media_info=info,
                             media_kind=s.media_kind, media_quality=s.media_quality)
        if not dlg.exec() or not dlg.result_value or not dlg.result_value.variant:
            return
        r = dlg.result_value
        v = r.variant
        s.media_kind, s.media_quality = v.kind, v.quality
        self.save_settings()
        start = r.action == "start"
        save_dir, name = os.path.dirname(r.path), os.path.basename(r.path)
        if replace is not None:
            if replace.status == Status.COMPLETE and os.path.exists(replace.path) and replace.path == r.path:
                os.remove(replace.path)
            self.manager.remove([replace.id])
        d = self.manager.add(info.url, name, save_dir, r.category, r.description, parent_page=info.url,
                             size=v.size, resumable=True, queue_name=r.queue if not start else "", start=start,
                             media=media.media_record(info, v))
        self.refresh_rows()
        if start:
            self.show_progress(d.id)

    def _unique_name(self, save_dir: str, name: str) -> str:
        taken = {d.filename for d in self.manager.downloads.values() if d.save_dir == save_dir}
        stem, ext = os.path.splitext(name)
        n, candidate = 2, name
        while candidate in taken or os.path.exists(os.path.join(save_dir, candidate)):
            candidate = f"{stem} ({n}){ext}"
            n += 1
        return candidate

    def _confirm_and_add(self, url, name, size, resumable, referer, description, parent_page,
                         username, password, replace: Download | None, headers: dict | None = None):
        start, queue_name = not self.settings.only_add_to_queue, ""
        cat = categories.classify(self.settings, name, url)
        save_dir, category = cat.save_dir, cat.name
        if replace is None:
            name = self._unique_name(save_dir, name)
        if self.settings.show_start_dialog:
            dlg = FileInfoDialog(self.settings, url, name, size, self._queue_names(), description, self,
                                 new_category=self.add_category)
            if not dlg.exec() or not dlg.result_value:
                return
            r = dlg.result_value
            save_dir, name, category, description = os.path.dirname(r.path), os.path.basename(r.path), r.category, r.description
            if r.action == "later":
                start, queue_name = False, r.queue
            else:
                start = True
            self.save_settings()
        if replace is not None:
            if replace.status == Status.COMPLETE and os.path.exists(replace.path) and replace.path == os.path.join(save_dir, name):
                os.remove(replace.path)
            self.manager.remove([replace.id])
        d = self.manager.add(url, name, save_dir, category, description, referer, parent_page, username, password,
                             size, resumable, queue_name, start, headers=headers)
        self.refresh_rows()
        if start:
            self.show_progress(d.id)

    def batch_download(self):
        dlg = BatchDialog(self)
        if dlg.exec():
            user, pw = dlg.auth.credentials()
            self._add_many([(u, "", "") for u in dlg.urls()], username=user, password=pw)

    def batch_from_clipboard(self):
        text = QApplication.clipboard().text()
        links = [(w, "", "a") for w in text.split() if URL_RE.match(w)]
        if not links:
            QMessageBox.information(self, "LID", "Cannot find any web address in the clipboard!")
            return
        dlg = LinksDialog(self.settings, links, self)
        if dlg.exec():
            self._add_choices(dlg.choices(), "", "")

    def run_grabber(self):
        url, ok = QInputDialog.getText(self, "Grabber", "Enter the address of the web page to explore:",
                                       text=QApplication.clipboard().text().strip() if URL_RE.match(
                                           QApplication.clipboard().text().strip()) else "https://")
        if not ok or not URL_RE.match(url.strip()):
            return
        url = url.strip()
        QApplication.setOverrideCursor(Qt.BusyCursor)

        def fetch():
            resp = self.manager.client().open(url, ranged=False)
            try:
                charset = resp.headers.get_content_charset() or "utf-8"
                return resp.geturl(), resp.read(8 * 1024 * 1024).decode(charset, "replace")
            finally:
                resp.close()

        def done(result, error):
            QApplication.restoreOverrideCursor()
            if error is not None:
                QMessageBox.warning(self, "Grabber", f"Cannot open the web page:\n{error}")
                return
            base, html = result
            links = extract_links(html, base)
            if not links:
                QMessageBox.information(self, "Grabber", "Cannot find the links!")
                return
            dlg = LinksDialog(self.settings, links, self)
            if dlg.exec():
                self._add_choices(dlg.choices(), base, base)

        run_async(fetch, done)

    def _add_choices(self, choices, referer, parent_page, headers: dict | None = None):
        for c in choices:
            name = self._unique_name(c.save_dir, c.filename)
            self.manager.add(c.url, name, c.save_dir, c.category, c.description, referer, parent_page,
                             queue_name=MAIN_QUEUE, headers=headers)
        if choices:
            self.handle_events(self.manager.start_queue(MAIN_QUEUE))
        self.refresh_rows()

    # ------------------------------------------------------------------ browser extension
    def handle_browser_request(self, req: dict):
        """A request from the LID browser extension, relayed by the native messaging host."""
        action = req.get("action")
        if action == "show":
            self.show_normal()
            return
        headers = {}
        if req.get("cookies"):
            headers["Cookie"] = req["cookies"]
        if req.get("user_agent"):
            headers["User-Agent"] = req["user_agent"]
        page = req.get("page_url") or ""
        referer = req.get("referer") or page
        if action == "download":
            url = req.get("url") or ""
            if URL_RE.match(url):
                self.add_url(url, referer=referer, parent_page=page, ask_address=False,
                             headers=headers, filename=req.get("filename") or "")
        elif action == "links":
            links = [(i["url"], i.get("text", ""), i.get("kind", "a")) for i in req.get("items", [])
                     if isinstance(i, dict) and URL_RE.match(i.get("url", ""))]
            if links:
                dlg = LinksDialog(self.settings, links, self)
                dlg.raise_()
                if dlg.exec():
                    self._add_choices(dlg.choices(), referer, page, headers)

    def _add_many(self, items, username="", password=""):
        for url, referer, desc in items:
            name = safe_filename(filename_from_url(url))
            cat = categories.classify(self.settings, name, url)
            self.manager.add(url, self._unique_name(cat.save_dir, name), cat.save_dir, cat.name, desc, referer,
                             username=username, password=password, queue_name=MAIN_QUEUE)
        self.refresh_rows()
        if items and QMessageBox.question(self, "LID", f"{len(items)} file(s) were added to the "
                                          f'"{MAIN_QUEUE}". Start the queue now?') == QMessageBox.Yes:
            self.handle_events(self.manager.start_queue(MAIN_QUEUE))

    # ------------------------------------------------------------------ download control
    def show_progress(self, download_id: int):
        if not self.settings.show_progress_dialog:
            return
        dlg = self.progress_dialogs.get(download_id)
        if dlg is None:
            dlg = ProgressDialog(self.manager, download_id, None)
            dlg.destroyed.connect(lambda *_: self.progress_dialogs.pop(download_id, None))
            self.progress_dialogs[download_id] = dlg
        dlg.show()
        dlg.raise_()

    def resume_selected(self):
        ids = self.target_ids()
        for i in ids:
            if self.manager.downloads[i].status != Status.COMPLETE and not self.manager.is_running(i):
                self.manager.start(i)
                if len(ids) == 1 and not self.details.isVisible():
                    self.show_progress(i)
        self.refresh_rows()

    def stop_selected(self):
        for i in self.target_ids():
            self.manager.stop(i)

    def redownload_selected(self):
        for i in self.target_ids():
            self.manager.redownload(i)
        self.refresh_rows()

    def remove_selected(self):
        ids = self.target_ids()
        if not ids:
            return
        box = QMessageBox(QMessageBox.Question, "Confirm deletion of downloads",
                          f"Are you sure you want to delete {len(ids)} file(s) from the download list?",
                          QMessageBox.Yes | QMessageBox.No, self)
        # Parented: PySide does not hand the check box to the message box, so a parentless one would be
        # freed with this Python variable and leave the box with a dangling pointer.
        also = QCheckBox("Also delete the downloaded files from disk", box)
        box.setCheckBox(also)
        answer, delete_files = box.exec(), also.isChecked()
        box.deleteLater()  # a parented message box would otherwise live as long as the window
        if answer == QMessageBox.Yes:
            self.manager.remove(ids, delete_files=delete_files)
            self.model.set_checked(ids, False)
            self.refresh_rows()

    def delete_completed(self):
        n = sum(1 for d in self.manager.downloads.values() if d.status == Status.COMPLETE)
        if n and QMessageBox.question(self, "Confirm deletion of downloads",
                                      f"Remove all {n} completed file(s) from the list?\n"
                                      "The files stay on the disk.") == QMessageBox.Yes:
            self.manager.remove_completed()
            self.refresh_rows()

    def open_with(self, d: Download):
        import shlex

        cmd, ok = QInputDialog.getText(self, "Open with", "Command (the file path is appended):")
        if ok and cmd.strip():
            subprocess.Popen(shlex.split(cmd) + [d.path], start_new_session=True)

    def properties(self, download_id: int):
        if PropertiesDialog(self.manager, download_id, self).exec():
            self.refresh_rows()

    # ------------------------------------------------------------------ queues & scheduler
    def _queue_names(self) -> list[str]:
        return [MAIN_QUEUE, SYNC_QUEUE] + sorted(n for n in self.manager.queues if n not in (MAIN_QUEUE, SYNC_QUEUE))

    def _fill_queue_menu(self, menu: QMenu, start: bool):
        menu.clear()
        for q in self._queue_names():
            a = menu.addAction(q, partial(self.start_queue if start else self.stop_queue, q))
            running = self.manager.queues[q].running
            a.setEnabled(not running if start else running)

    def start_queue(self, name):
        self.handle_events(self.manager.start_queue(name))
        self.refresh_rows()

    def stop_queue(self, name):
        self.manager.stop_queue(name)

    def open_scheduler(self, queue: str = MAIN_QUEUE):
        SchedulerDialog(self.manager, self, queue, on_events=self.handle_events).exec()
        self.rebuild_tree()
        self.refresh_rows()

    # ------------------------------------------------------------------ search
    def show_searchbar(self):
        self.searchbar.setVisible(True)
        self.searchbar.edit.setText(self.search.query)
        self.searchbar.edit.setFocus()
        self.searchbar.edit.selectAll()

    def apply_search(self):
        self.search.query = self.searchbar.edit.text()
        self.search.active = bool(self.search.query)
        try:
            self.search.validate()
        except SearchSettingsError as e:
            QMessageBox.information(self, "LID Search settings", str(e))
        self.settings.search_flags, self.settings.search_string = int(self.search.flags), self.search.query
        self.finder.reset()
        self.refresh_rows()

    def search_settings(self):
        if FindDialog(self.search, self, "LID Search settings", show_query=False).exec():
            self.settings.search_flags = int(self.search.flags)
            if self.search.active:
                self.refresh_rows()

    def close_searchbar(self):
        self.searchbar.setVisible(False)
        self.search.active = False
        self.refresh_rows()
        self.view.setFocus()

    def find_next(self):
        if not self.search.query:
            if not FindDialog(self.search, self).exec():
                return
            self.settings.search_flags, self.settings.search_string = int(self.search.flags), self.search.query
        row = self.finder.next(self.model.rows)
        if row is None:
            QMessageBox.information(self, "Find file", "No such files found")
            return
        idx = self.model.index(row, 0)
        self.view.selectionModel().setCurrentIndex(idx, QItemSelectionModel.ClearAndSelect | QItemSelectionModel.Rows)
        self.view.scrollTo(idx)

    # ------------------------------------------------------------------ settings & view
    def open_options(self):
        if OptionsDialog(self.settings, self).exec():
            self.apply_appearance()
            self.save_settings()
            self.rebuild_tree()
            self.refresh_rows()

    def apply_appearance(self):
        """Apply what Options → Appearance may have changed."""
        s = self.settings
        self.apply_theme()
        self.set_toolbar_labels(s.toolbar_labels)
        self.set_menubar_mode(s.menubar)
        self.toggle_categories(not s.show_categories)
        self.set_details_visible(s.show_details)
        self.set_statusbar_visible(s.show_statusbar)
        f = QFont()
        self.view.setFont(f if s.list_font and f.fromString(s.list_font) else QApplication.font())
        if s.tray_icon and self.tray is None:
            self._build_tray()
        elif not s.tray_icon and self.tray is not None:
            self.tray.hide()
            self.tray = None

    def set_limiter(self, on: bool):
        self.manager.set_speed_limiter(on)
        self._sync_limiter_actions()
        self.save_settings()

    def _sync_limiter_actions(self):
        on = self.settings.speed_limiter_on
        self.a_limit_on.setChecked(on)
        self.a_limit_off.setChecked(not on)

    def limiter_settings(self):
        if SpeedLimiterDialog(self.settings, self).exec():
            self.manager.set_speed_limiter(self.settings.speed_limiter_on)
            self.save_settings()

    def toggle_categories(self, hide: bool):
        self.settings.show_categories = not hide
        self.cat_panel.setVisible(not hide)
        self.a_hide_cats.setChecked(hide)

    def set_toolbar_labels(self, on: bool):
        self.settings.toolbar_labels = on
        for b in self._toolbar_buttons:
            primary = b.objectName() == "primary"
            b.setToolButtonStyle(Qt.ToolButtonTextBesideIcon if on or primary else Qt.ToolButtonIconOnly)
        (self.a_labels if on else self.a_icons_only).setChecked(True)

    def set_menubar_mode(self, mode: str):
        """native: the desktop's global menu (Qt falls back to the window when there is none);
        custom: a menu bar in the window only; both: the global menu and the window."""
        if mode not in self.menubar_actions:
            mode = "native"
        self.settings.menubar = mode
        global_menu = theme.global_menu_available()
        self.menuBar().setNativeMenuBar(mode != "custom")
        self.menuBar().setVisible(mode != "custom")
        inline = mode == "custom" or (mode == "both" and global_menu)
        self.menu_toolbar.setVisible(inline)
        # Qt hides every new QMenuBar while a global menu is around, even one set to non-native; in a
        # toolbar that hides the widget's action, which is what decides whether the widget shows.
        self._menubar_action.setVisible(inline)
        self.menubar_actions[mode].setChecked(True)

    def set_details_visible(self, on: bool):
        self.settings.show_details = on
        self.details.setVisible(on)
        self.a_details.setChecked(on)
        if on:
            self._show_details()

    def set_statusbar_visible(self, on: bool):
        self.settings.show_statusbar = on
        self.statusBar().setVisible(on)
        self.a_statusbar.setChecked(on)
        self.update_statusbar()

    def _show_details(self):
        idx = self.view.currentIndex()
        d_id = self.model.rows[idx.row()].id if idx.isValid() and idx.row() < len(self.model.rows) else None
        if d_id is None and self.model.rows:
            sel = self.selected_ids()
            d_id = sel[0] if sel else None
        self.details.show_download(d_id)

    def customize_columns(self):
        if ColumnsDialog(self.settings, self).exec():
            self.model.set_columns()
            self.refresh_rows()
            self.apply_column_widths()

    def choose_theme(self, key: str):
        if key == "system":
            self.settings.follow_system = True
        else:
            self.settings.follow_system = False
            self.settings.theme = key
        self.apply_theme()
        self.save_settings()

    def _sync_theme_actions(self):
        key = "system" if self.settings.follow_system else self.settings.theme
        if key in self.theme_actions:
            self.theme_actions[key].setChecked(True)

    def apply_theme(self):
        theme.apply(theme.resolve(self.settings))
        self.list_delegate.row_height = ROW_HEIGHT.get(self.settings.density, 32)
        self.view.doItemsLayout()
        self._sync_theme_actions()
        if hasattr(self, "tree"):
            self.rebuild_tree()

    def choose_font(self):
        ok, font = QFontDialog.getFont(self.view.font(), self)
        if ok:
            self.view.setFont(font)
            self.settings.list_font = font.toString()

    def reset_font(self):
        self.view.setFont(QApplication.font())
        self.settings.list_font = ""

    def about(self):
        QMessageBox.about(self, f"About {APP_NAME}",
                          f"<b>{APP_LONG_NAME} {__version__}</b><br>{APP_NAME} · package {PACKAGE_NAME}<br><br>"
                          "A fast download manager for Linux: multi-connection downloads with dynamic "
                          "segmentation, queues, scheduler, categories, YouTube, clipboard monitoring and search."
                          "<br><br>MIT License.<br><br><small>LID is an independent project, not affiliated with "
                          "Tonec Inc. or Internet Download Manager.</small>")

    # ------------------------------------------------------------------ import / export
    def export_ef2(self):
        path, _ = QFileDialog.getSaveFileName(self, "Export download list", "downloads.ef2", "Export files (*.ef2)")
        if not path:
            return
        with open(path, "w", encoding="utf-8", newline="\r\n") as f:
            for d in sorted(self.manager.downloads.values(), key=lambda d: d.id):
                f.write("<\n" + d.url + "\n")
                if d.referer:
                    f.write(f"referer: {d.referer}\n")
                f.write(">\n")

    def export_text(self):
        path, _ = QFileDialog.getSaveFileName(self, "Export download list", "downloads.txt", "Text files (*.txt)")
        if path:
            with open(path, "w", encoding="utf-8") as f:
                f.write("\n".join(d.url for d in sorted(self.manager.downloads.values(), key=lambda d: d.id)) + "\n")

    @staticmethod
    def parse_ef2(text: str) -> list[tuple[str, str]]:
        out, block = [], None
        for line in text.splitlines():
            line = line.strip()
            if line == "<":
                block = []
            elif line == ">" and block is not None:
                if block:
                    url = block[0]
                    ref = next((b.split(":", 1)[1].strip() for b in block[1:] if b.lower().startswith("referer:")), "")
                    out.append((url, ref))
                block = None
            elif block is not None and line:
                block.append(line)
        return out

    def import_ef2(self):
        path, _ = QFileDialog.getOpenFileName(self, "Import", "", "Export files (*.ef2);;All files (*)")
        if path:
            with open(path, encoding="utf-8", errors="replace") as f:
                items = self.parse_ef2(f.read())
            self._add_many([(u, r, "") for u, r in items])

    def import_text(self):
        path, _ = QFileDialog.getOpenFileName(self, "Import", "", "Text files (*.txt);;All files (*)")
        if path:
            with open(path, encoding="utf-8", errors="replace") as f:
                urls = [w for w in f.read().split() if URL_RE.match(w)]
            self._add_many([(u, "", "") for u in urls])

    # ------------------------------------------------------------------ events
    def tick(self):
        self.handle_events(self.manager.poll())
        self.refresh_rows()
        self.update_tree_counts()
        if self.details.isVisible():
            if self.details.download_id is None:
                self._show_details()
            self.details.refresh()
        self.update_statusbar()
        if self.tray:
            active = len(self.manager.tasks)
            self.tray.setToolTip(f"LID - {active} active download(s)" if active else "LID")

    def handle_events(self, events: list[Event]):
        for e in events:
            if e.kind == "complete":
                self._download_complete(e.download_id)
            elif e.kind == "error":
                dlg = self.progress_dialogs.get(e.download_id)
                if dlg:
                    dlg.refresh()
                elif self.tray and e.download_id in self.manager.downloads:
                    d = self.manager.downloads[e.download_id]
                    self.tray.showMessage("Download failed", f"{d.filename}\n{e.message}", QSystemTrayIcon.Warning)
            elif e.kind == "queue_done" and e.queue in self.manager.queues:
                self._queue_done(e.queue)
        if events:
            self.refresh_rows()

    def _download_complete(self, download_id: int):
        d = self.manager.downloads.get(download_id)
        if d is None:
            return
        opts = {"show_complete": self.settings.show_complete_dialog, "exit": False, "shutdown": False}
        dlg = self.progress_dialogs.pop(download_id, None)
        if dlg is not None:
            opts = dlg.completion_options()
            dlg.close()
        if d.queue and self.manager.queues.get(d.queue) and self.manager.queues[d.queue].running:
            opts["show_complete"] = False  # these settings don't apply to queue processing
        if opts["show_complete"]:
            cd = CompleteDialog(d, None)
            cd.finished.connect(lambda *_: self._complete_closed(cd))
            cd.show()
        elif self.tray:
            self.tray.showMessage("Download complete", d.filename, QSystemTrayIcon.Information, 3000)
        if opts["shutdown"]:
            self.shutdown_computer()
        elif opts["exit"]:
            self.quit_app()

    def _complete_closed(self, dlg: CompleteDialog):
        if dlg.dont_show.isChecked():
            self.settings.show_complete_dialog = False
            self.save_settings()

    def _queue_done(self, name: str):
        s = self.manager.queues[name].schedule
        if self.tray:
            self.tray.showMessage("LID", f'"{name}" processing stopped/finished', QSystemTrayIcon.Information, 3000)
        if s.open_when_done:
            open_path(s.open_when_done)
        if s.shutdown_when_done:
            self.shutdown_computer()
        elif s.exit_when_done:
            self.quit_app()

    def shutdown_computer(self):
        box = QMessageBox(QMessageBox.Warning, "Shut down",
                          "LID has finished downloading. The computer will be turned off in 30 seconds.",
                          QMessageBox.Cancel, self)
        left = [30]

        def countdown():
            left[0] -= 1
            box.setText(f"LID has finished downloading. The computer will be turned off in {left[0]} seconds.")
            if left[0] <= 0:
                timer.stop()
                box.accept()

        timer = QTimer(box)
        timer.timeout.connect(countdown)
        timer.start(1000)
        if box.exec() == QMessageBox.Cancel and left[0] > 0:
            return
        self.manager.shutdown()
        subprocess.Popen(["systemctl", "poweroff"])

    # ------------------------------------------------------------------ clipboard & drag-drop
    def _clipboard_changed(self):
        if not self.settings.monitor_clipboard:
            return
        text = QApplication.clipboard().text().strip()
        if text == self._last_clip or not URL_RE.match(text):
            return
        self._last_clip = text
        host = (urlsplit(text).hostname or "").lower()
        if any(host.endswith(s.lower()) for s in self.settings.no_auto_sites.split()):
            return
        if media.is_youtube(text):
            self.add_url(text, ask_address=False)
            return
        name = filename_from_url(text)
        if categories.is_auto_capture_type(self.settings, name):
            self.add_url(text, ask_address=False)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls() or e.mimeData().hasText():
            e.acceptProposedAction()

    def dropEvent(self, e):
        md = e.mimeData()
        urls = [u.toString() for u in md.urls()] if md.hasUrls() else md.text().split()
        urls = [u for u in urls if URL_RE.match(u)]
        if len(urls) == 1:
            self.add_url(urls[0], ask_address=False)
        elif urls:
            self._add_many([(u, "", "") for u in urls])

    # ------------------------------------------------------------------ window lifecycle
    def show_normal(self):
        self.showNormal()
        self.raise_()
        self.activateWindow()

    def save_settings(self):
        self.settings.window_geometry = bytes(self.saveGeometry().toBase64()).decode()
        settings_mod.save(self.settings)

    def closeEvent(self, e):
        if self.tray and self.settings.close_to_tray and not self.really_quit:
            self.hide()
            e.ignore()
            return
        self.save_settings()
        for dlg in list(self.progress_dialogs.values()):
            dlg.close()
        self.manager.shutdown()
        e.accept()
        QApplication.quit()

    def quit_app(self):
        self.really_quit = True
        self.close()


def _iter_items(tree: QTreeWidget):
    stack = [tree.topLevelItem(i) for i in range(tree.topLevelItemCount() - 1, -1, -1)]
    while stack:
        item = stack.pop()
        yield item
        stack.extend(item.child(i) for i in range(item.childCount() - 1, -1, -1))
