"""Details pane under the download list: status, connections and speed limiter of the current download."""

from __future__ import annotations

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QCheckBox,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QProgressBar,
    QSizePolicy,
    QSpinBox,
    QStackedWidget,
    QTabBar,
    QToolButton,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ..core import fmt
from ..core.manager import DownloadManager
from ..core.models import Status
from .dialogs.progress import SegmentBar
from .icons import bind, category_icon, icon
from .util import open_folder, open_path
from .widgets import InfoPillDelegate, ToolButton, mono_font, small_header


def _label(text: str = "", name: str = "", mono: bool = False, scale: float = 1.0, bold: bool = False,
           semibold: bool = False) -> QLabel:
    lab = QLabel(text)
    if name:
        lab.setObjectName(name)
    f = mono_font(lab.font()) if mono else QFont(lab.font())
    if scale != 1.0:
        f.setPointSizeF(lab.font().pointSizeF() * scale)
    f.setWeight(QFont.Bold if bold else QFont.DemiBold if semibold else QFont.Normal)
    lab.setFont(f)
    return lab


MIN_WIDTH = 560  # the tabs, the file block and a usable progress column
WIDE = 960  # from here the transfer rate / time left / ... column fits as well


class DetailsPane(QFrame):
    closed = Signal()
    open_window = Signal(int)  # the download's own status window

    def __init__(self, manager: DownloadManager, parent=None):
        super().__init__(parent)
        self.setObjectName("details")
        self.manager = manager
        self.download_id: int | None = None

        head = QFrame()
        head.setObjectName("detailsTabs")
        head.setFixedHeight(30)
        top = QHBoxLayout(head)
        top.setContentsMargins(8, 0, 8, 0)
        top.setSpacing(2)
        self.tabs = QTabBar()
        for t in ("Download status", "Connections", "Speed Limiter"):
            self.tabs.addTab(t)
        self.tabs.setDrawBase(False)
        top.addWidget(self.tabs)
        hint = _label("View → Details pane (F9)", "faint", scale=0.96)
        hint.setMinimumWidth(1)
        hint.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        hint.setAlignment(Qt.AlignRight | Qt.AlignVCenter)
        top.addWidget(hint, 1)
        top.addSpacing(6)
        pop = QToolButton()
        pop.setObjectName("close")
        bind(pop, "open-window")
        pop.setIconSize(QSize(13, 13))
        pop.setToolTip("Open in a window")
        pop.clicked.connect(lambda: self.download_id is not None and self.open_window.emit(self.download_id))
        top.addWidget(pop)
        close = QToolButton()
        close.setObjectName("close")
        bind(close, "close")
        close.setIconSize(QSize(13, 13))
        close.setToolTip("Hide the details pane")
        close.clicked.connect(self.closed.emit)
        top.addWidget(close)

        self.pages = QStackedWidget()
        self.pages.addWidget(self._status_page())
        self.pages.addWidget(self._connections_page())
        self.pages.addWidget(self._limiter_page())
        self.empty = _label("Select a download to see its progress, connections and speed limit here.", "faint")
        self.empty.setAlignment(Qt.AlignCenter)
        self.empty.setWordWrap(True)
        self.pages.addWidget(self.empty)
        self.tabs.currentChanged.connect(lambda i: self.pages.setCurrentIndex(i) if self.download_id is not None else None)

        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        lay.addWidget(head)
        lay.addWidget(self.pages, 1)
        self.setMinimumHeight(170)
        # Set outright so the stats column below does not hold the pane (and the
        # sidebar next to it) at its full width; resizeEvent drops that column instead.
        self.setMinimumWidth(MIN_WIDTH)
        self.show_download(None)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.stats_column.setVisible(e.size().width() >= WIDE)

    @staticmethod
    def _button(text: str, icon_name: str = "") -> ToolButton:
        b = ToolButton(pad=14, gap=6)
        b.setObjectName("compact")
        b.setText(text)
        if icon_name:
            bind(b, icon_name)
        b.setIconSize(QSize(13, 13))
        b.setToolButtonStyle(Qt.ToolButtonTextBesideIcon)
        b.setFixedHeight(30)
        b.setCursor(Qt.PointingHandCursor)
        return b

    # -- pages
    def _status_page(self) -> QWidget:
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(18, 14, 18, 14)
        lay.setSpacing(22)

        left = QHBoxLayout()
        left.setSpacing(12)
        tile = QFrame()
        tile.setObjectName("tile")
        tile.setFixedSize(52, 52)
        tl = QVBoxLayout(tile)
        tl.setContentsMargins(0, 0, 0, 0)
        self.pic = QLabel()
        self.pic.setAlignment(Qt.AlignCenter)
        tl.addWidget(self.pic)
        left.addWidget(tile, 0, Qt.AlignTop)
        info = QVBoxLayout()
        info.setSpacing(4)
        self.name = _label(bold=True, scale=1.125)
        self.url = _label(name="link", mono=True, scale=0.92)
        self.state = _label(name="muted", scale=0.96)
        for lab in (self.name, self.url, self.state):
            lab.setMinimumWidth(1)
            info.addWidget(lab)
        buttons = QHBoxLayout()
        buttons.setSpacing(6)
        self.pause = self._button("Pause", "stop")
        self.pause.clicked.connect(self._pause_resume)
        self.open_folder = self._button("Open folder")
        self.open_folder.clicked.connect(self._open_folder)
        buttons.addWidget(self.pause)
        buttons.addWidget(self.open_folder)
        buttons.addStretch(1)
        info.addSpacing(8)
        info.addLayout(buttons)
        info.addStretch(1)
        left.addLayout(info, 1)
        lw = QWidget()
        lw.setLayout(left)
        lw.setFixedWidth(290)
        lay.addWidget(lw)

        mid = QVBoxLayout()
        mid.setSpacing(12)
        row = QHBoxLayout()
        row.setSpacing(8)
        self.pct = _label(mono=True, scale=2.0, bold=True, name="bigNumber")
        self.of = _label(name="muted")
        row.addWidget(self.pct)
        row.addWidget(self.of, 0, Qt.AlignBottom)
        row.addStretch(1)
        mid.addLayout(row)
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.bar.setTextVisible(False)
        self.bar.setFixedHeight(10)
        mid.addWidget(self.bar)
        self.of.setMinimumWidth(1)
        caption = _label("Start positions and download progress by connections", "muted", scale=0.96, semibold=True)
        caption.setWordWrap(True)
        mid.addWidget(caption)
        self.segments = SegmentBar()
        mid.addWidget(self.segments)
        mid.addStretch(1)
        lay.addLayout(mid, 1)

        grid = QGridLayout()
        grid.setHorizontalSpacing(18)
        grid.setVerticalSpacing(12)
        self.stats = {}
        for i, (key, title) in enumerate((("rate", "Transfer rate"), ("left", "Time left"), ("done", "Downloaded"),
                                          ("conns", "Connections"), ("resume", "Resume"))):
            cell = QVBoxLayout()
            cell.setSpacing(3)
            cell.addWidget(_label(title.upper(), "faint", scale=0.92, semibold=True))
            v = _label(mono=True, bold=True, scale=1.08)
            cell.addWidget(v)
            grid.addLayout(cell, *divmod(i, 2))
            self.stats[key] = v
        self.stats["resume"].setObjectName("okText")
        self.stats_column = gw = QWidget()
        gw.setLayout(grid)
        gw.setFixedWidth(330)
        lay.addWidget(gw, 0, Qt.AlignTop)
        return w

    def _connections_page(self) -> QWidget:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.setContentsMargins(14, 10, 14, 10)
        self.conns = QTreeWidget()
        self.conns.setHeaderLabels(["N.", "DOWNLOADED", "SPEED", "INFO"])
        self.conns.setRootIsDecorated(False)
        self.conns.setAlternatingRowColors(True)
        for col, width in ((0, 50), (1, 120), (2, 110)):
            self.conns.setColumnWidth(col, width)
        self.conns.setItemDelegateForColumn(3, InfoPillDelegate(self.conns))
        small_header(self.conns)
        lay.addWidget(self.conns)
        return w

    def _limiter_page(self) -> QWidget:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.setContentsMargins(18, 14, 18, 14)
        self.use_limit = QCheckBox("Use Speed Limiter for this download")
        row = QHBoxLayout()
        row.addWidget(QLabel("Maximum download speed:"))
        self.limit = QSpinBox()
        self.limit.setRange(1, 10_000_000)
        self.limit.setMinimumWidth(110)
        row.addWidget(self.limit)
        row.addWidget(QLabel("KBytes/sec"))
        row.addStretch(1)
        lay.addWidget(self.use_limit)
        lay.addLayout(row)
        lay.addStretch(1)
        self.use_limit.toggled.connect(self._apply_limit)
        self.limit.valueChanged.connect(self._apply_limit)
        return w

    # -- behaviour
    def show_download(self, download_id: int | None) -> None:
        if download_id is not None and download_id not in self.manager.downloads:
            download_id = None
        changed = download_id != self.download_id
        self.download_id = download_id
        if download_id is None:
            self.pages.setCurrentWidget(self.empty)
            return
        if changed:
            d = self.manager.downloads[download_id]
            self.pic.setPixmap(icon(category_icon(d.category)).pixmap(30, 30))
            for wdg in (self.use_limit, self.limit):
                wdg.blockSignals(True)
            self.use_limit.setChecked(d.speed_limit_kbps > 0)
            self.limit.setValue(d.speed_limit_kbps or self.manager.settings.speed_limit_kbps)
            for wdg in (self.use_limit, self.limit):
                wdg.blockSignals(False)
            self.pages.setCurrentIndex(self.tabs.currentIndex())
        self.refresh()

    def refresh(self) -> None:
        if self.download_id is None or self.download_id not in self.manager.downloads:
            self.show_download(None)
            return
        if not self.isVisible():
            return
        d = self.manager.downloads[self.download_id]
        task = self.manager.task(d.id)
        running = self.manager.is_running(d.id)
        snap = task.snapshot() if task else {"segments": d.segments, "connections": [], "rate": 0.0, "time_left": -1}
        self.name.setText(self._elide(self.name, d.filename, Qt.ElideRight))
        self.url.setText(self._elide(self.url, d.url.split("://", 1)[-1], Qt.ElideRight))
        self.url.setToolTip(d.url)
        state = {Status.COMPLETE: "Complete", Status.ERROR: f"Error: {d.error}", Status.PAUSED: "Paused",
                 Status.ASSEMBLING: "Assembling…", Status.CONNECTING: "Connecting…"}.get(d.status)
        if state is None:
            state = "Receiving data…" if running else ("Queued" if not d.downloaded else "Paused")
        self.state.setText(self._elide(self.state, f"{state} · {d.category}"))
        if self.pause.text() != ("Pause" if running else "Resume"):
            self.pause.setText("Pause" if running else "Resume")
            bind(self.pause, "stop" if running else "resume")
            self.pause.updateGeometry()
        self.pause.setEnabled(d.status != Status.COMPLETE)
        self.pct.setText(f"{d.progress * 100:.1f}%" if d.size > 0 else fmt.size(d.downloaded))
        self.of.setText(f"{fmt.size(d.downloaded)} of {fmt.size(d.size)}" if d.size > 0 else "")
        self.bar.setValue(int(d.progress * 1000))
        self.segments.set_segments(d.size, snap["segments"])
        conns = snap["connections"]
        alive = sum(1 for c in conns if c[3])
        finished = sum(1 for s in snap["segments"] if s.end >= 0 and s.pos >= s.end)
        self.stats["rate"].setText(fmt.rate(snap["rate"]) if running else "—")
        self.stats["left"].setText(fmt.duration(snap["time_left"]) if running and snap["time_left"] >= 0 else "—")
        self.stats["done"].setText(f"{fmt.size(d.downloaded)} / {fmt.size(d.size)}" if d.size > 0 else fmt.size(d.downloaded))
        self.stats["conns"].setText(f"{alive} active · {finished} done" if conns or finished else "—")
        self.stats["resume"].setText({True: "Yes", False: "No"}.get(d.resumable, "Unknown"))
        while self.conns.topLevelItemCount() > len(conns):
            self.conns.takeTopLevelItem(self.conns.topLevelItemCount() - 1)
        for i, (n, received, info, is_alive, conn_rate) in enumerate(conns):
            item = self.conns.topLevelItem(i) or QTreeWidgetItem(self.conns)
            item.setText(0, str(n))
            item.setText(1, fmt.size(received))
            item.setText(2, fmt.rate(conn_rate) if is_alive and running else "")
            item.setText(3, info if is_alive or running else "")

    @staticmethod
    def _elide(label: QLabel, text: str, mode=Qt.ElideMiddle) -> str:
        width = max(80, label.width())
        return label.fontMetrics().elidedText(text, mode, width)

    def _pause_resume(self) -> None:
        if self.download_id is None:
            return
        if self.manager.is_running(self.download_id):
            self.manager.stop(self.download_id)
        else:
            self.manager.start(self.download_id)
        self.refresh()

    def _open_folder(self) -> None:
        if self.download_id is None:
            return
        d = self.manager.downloads[self.download_id]
        open_folder(d.path) if d.status == Status.COMPLETE else open_path(d.save_dir)

    def _apply_limit(self) -> None:
        if self.download_id is None:
            return
        kbps = self.limit.value() if self.use_limit.isChecked() else 0
        task = self.manager.task(self.download_id)
        if task:
            task.set_speed_limit(kbps)
        self.manager.downloads[self.download_id].speed_limit_kbps = kbps
