"""Dialogs for adding downloads: address, file info, duplicate, batch and link list."""

from __future__ import annotations

import html
import os
import re
import string
from dataclasses import dataclass
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit

from PySide6.QtCore import QSize, Qt
from PySide6.QtGui import QColor, QGuiApplication
from PySide6.QtWidgets import (
    QAbstractItemView,
    QButtonGroup,
    QCheckBox,
    QComboBox,
    QDialog,
    QFileDialog,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMenu,
    QPushButton,
    QRadioButton,
    QSpinBox,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from ...core import categories, fmt, media
from ...core.engine import filename_from_url, safe_filename
from ...core.settings import Settings
from .. import theme
from ..icons import bind, category_icon, icon
from ..widgets import (CheckHeader, TypePillDelegate, caption, form_label, form_row, link_field, mono_font, panel,
                       pill_label, primary, shell)

URL_RE = re.compile(r"^(https?|ftp)://\S+$", re.I)


def link_combo(combo: QComboBox) -> QComboBox:
    """An editable address combo: monospaced, with a link icon in front. The font comes from the
    style sheet: a font set on a QComboBox is dropped when the combo is put into a layout."""
    bind(combo.lineEdit().addAction(icon("link"), QLineEdit.LeadingPosition), "link")
    combo.setProperty("mono", True)
    return combo


def clipboard_url() -> str:
    text = QGuiApplication.clipboard().text().strip()
    return text if URL_RE.match(text) else ""


def queue_menu(parent: QWidget, queues: list[str], at) -> str | None:
    menu = QMenu(parent)
    for q in queues:
        menu.addAction(q).setData(q)
    act = menu.exec(at)
    return act.data() if act else None


class AuthBox(QFrame):
    """"Use authorization" check box with Login / Password. As a panel the fields are always there
    (Add URL); ``compact`` is a bare check box whose fields show once it is checked (Batch)."""

    def __init__(self, parent=None, compact: bool = False):
        super().__init__(parent)
        self.compact = compact
        lay = QVBoxLayout(self)
        if compact:
            lay.setContentsMargins(0, 0, 0, 0)
        else:
            self.setObjectName("panel")
            lay.setContentsMargins(14, 14, 14, 14)
            lay.setSpacing(12)
        self.use = QCheckBox("Use &authorization")
        lay.addWidget(self.use)
        self.fields = QWidget()
        row = QHBoxLayout(self.fields)
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(10)
        self.login = QLineEdit()
        self.password = QLineEdit()
        self.password.setEchoMode(QLineEdit.Password)
        self.labels = (form_label("Login"), form_label("Password"))
        row.addWidget(self.labels[0])
        row.addWidget(self.login, 1)
        row.addWidget(self.labels[1])
        row.addWidget(self.password, 1)
        lay.addWidget(self.fields)
        self.use.toggled.connect(self._toggled)
        self._toggled(False)

    def _toggled(self, on: bool) -> None:
        for w in (self.login, self.password, *self.labels):
            w.setEnabled(on)
        if self.compact:
            self.fields.setVisible(on)

    def isChecked(self) -> bool:
        return self.use.isChecked()

    def setChecked(self, on: bool) -> None:
        self.use.setChecked(on)

    def credentials(self) -> tuple[str, str]:
        return (self.login.text(), self.password.text()) if self.isChecked() else ("", "")


class AddUrlDialog(QDialog):
    """The "Enter new address to download" dialog."""

    history: list[str] = []

    def __init__(self, parent=None, url: str = ""):
        super().__init__(parent)
        self.setWindowTitle("Enter new address to download")
        self.setMinimumWidth(560)  # height follows from the layout at this width
        body, footer = shell(self, "add-url", "Enter new address to download",
                             "Paste a link. LID reads the file name and size next.")
        self.address = QComboBox()
        self.address.setEditable(True)
        self.address.addItems(self.history)
        self.address.setCurrentText(url or clipboard_url())
        self.address.lineEdit().selectAll()
        link_combo(self.address)
        body.addLayout(form_row("Address", self.address, width=64))
        self.youtube_hint = QWidget()
        hint = QHBoxLayout(self.youtube_hint)
        hint.setContentsMargins(72, 0, 0, 0)
        pic = QLabel()
        pic.setPixmap(icon("category-video").pixmap(16, 16))
        hint.addWidget(pic)
        hint.addWidget(QLabel("<b>YouTube video detected</b> — choose the resolution or audio only "
                              "in the next window."), 1)
        body.addWidget(self.youtube_hint)
        self.auth = AuthBox()
        body.addWidget(self.auth)
        ok = primary(QPushButton("O&K"))
        ok.setDefault(True)
        cancel = QPushButton("&Cancel")
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        footer.addStretch(1)
        footer.addWidget(cancel)
        footer.addWidget(ok)
        self.address.currentTextChanged.connect(self._detect)
        self._detect(self.address.currentText())

    def _detect(self, text: str) -> None:
        youtube = media.is_youtube(text)
        self.youtube_hint.setVisible(youtube)
        self.auth.setVisible(not youtube)

    def url(self) -> str:
        return self.address.currentText().strip()

    def accept(self):
        url = self.url()
        if not URL_RE.match(url):
            self.address.setFocus()
            self.address.lineEdit().selectAll()
            return
        if url in self.history:
            self.history.remove(url)
        self.history.insert(0, url)
        del self.history[20:]
        super().accept()


@dataclass
class FileInfoResult:
    action: str  # start | later
    path: str
    category: str
    description: str
    queue: str = ""
    variant: media.Variant | None = None


class FileInfoDialog(QDialog):
    """The "Download File Info" dialog."""

    def __init__(self, settings: Settings, url: str, filename: str, size: int, queues: list[str],
                 description: str = "", parent=None, new_category=None,
                 media_info: media.MediaInfo | None = None, media_kind: str = "video", media_quality: str = "",
                 resumable: bool | None = None):
        super().__init__(parent)
        self.media_info = media_info
        self.variant: media.Variant | None = None
        self.settings = settings
        self.queues = queues
        self.filename = filename
        self.new_category = new_category
        self.result_value: FileInfoResult | None = None
        self.setWindowTitle("Download File Info")
        self.setMinimumWidth(720)  # height follows from the layout at this width
        self.setMaximumWidth(1100)
        body, footer = shell(self, "folder", "Download File Info", filename, tile=True)

        self.url = QLineEdit(url)
        self.url.setFont(mono_font(self.url.font()))
        self.category = QComboBox()
        for c in settings.categories:
            self.category.addItem(icon(category_icon(c.name)), c.name)
        self.category.setMinimumWidth(200)
        add_cat = QPushButton("+")
        add_cat.setObjectName("small")
        add_cat.setFixedWidth(34)
        add_cat.setToolTip("Add category")
        add_cat.clicked.connect(self._add_category)
        self.save_as = QComboBox()
        self.save_as.setEditable(True)
        self.save_as.setSizeAdjustPolicy(QComboBox.AdjustToMinimumContentsLengthWithIcon)
        self.save_as.setMinimumContentsLength(30)
        browse = QPushButton("...")
        browse.setObjectName("small")
        browse.setFixedWidth(34)
        browse.clicked.connect(self._browse)
        self.remember = QCheckBox()
        self.description = QLineEdit(description)

        form = QVBoxLayout()
        form.setSpacing(10)
        form.addLayout(form_row("URL", self.url, width=78))
        form.addLayout(form_row("Category", self.category, add_cat, QWidget(), width=78))
        form.addLayout(form_row("Save As", self.save_as, browse, width=78, stretch_last=False))
        remember = QHBoxLayout()
        remember.addSpacing(86)
        remember.addWidget(self.remember)
        form.addLayout(remember)
        if media_info:
            self.url.setReadOnly(True)
            form.addWidget(self._media_box(media_info))
        form.addLayout(form_row("Description", self.description, width=78))

        side = QVBoxLayout()
        side.setContentsMargins(0, 0, 0, 0)
        side.setSpacing(8)
        tile = QFrame()
        tile.setObjectName("tileLarge")
        tile.setFixedSize(72, 72)
        tl = QVBoxLayout(tile)
        tl.setContentsMargins(0, 0, 0, 0)
        self.pic = pic = QLabel()
        pic.setAlignment(Qt.AlignCenter)
        tl.addWidget(pic)
        self.size_label = QLabel(fmt.size(size) or "(unknown)")
        self.size_label.setFont(mono_font(self.size_label.font()))
        f = self.size_label.font()
        f.setBold(True)
        self.size_label.setFont(f)
        self.size_label.setAlignment(Qt.AlignCenter)
        side.addSpacing(4)
        side.addWidget(tile, 0, Qt.AlignHCenter)
        side.addWidget(self.size_label)
        if resumable is not None:
            side.addWidget(pill_label("✓ Resumable", "ok") if resumable else pill_label("Not resumable", "muted"),
                           0, Qt.AlignHCenter)
        side.addStretch(1)
        sw = QWidget()
        sw.setLayout(side)
        sw.setFixedWidth(116)

        top = QHBoxLayout()
        top.setSpacing(16)
        top.addLayout(form, 1)
        top.addWidget(sw)
        body.addLayout(top)

        later = QPushButton("Download &Later")
        later.setIcon(icon("queue"))
        start = primary(QPushButton("&Start Download"))
        start.setIcon(icon("add-white"))
        for b in (later, start):
            b.setIconSize(QSize(15, 15))
        cancel = QPushButton("&Cancel")
        start.setDefault(True)
        later.clicked.connect(self._later)
        start.clicked.connect(lambda: self._finish("start", ""))
        cancel.clicked.connect(self.reject)
        footer.addStretch(1)
        for b in (later, start, cancel):
            footer.addWidget(b)
        start.setFocus()  # once it is in the dialog; Enter starts the download

        self.category.currentTextChanged.connect(self._category_changed)
        cat = categories.classify(settings, filename, url)
        self.category.setCurrentText(cat.name)
        self._category_changed(cat.name)
        if media_info:
            has_kind = any(v.kind == media_kind for v in media_info.variants)
            kind = media_kind if has_kind else media_info.variants[0].kind
            (self.kind_video if kind == "video" else self.kind_audio).setChecked(True)
            self._fill_quality(media_quality)

    # -- YouTube variant picker (shown only for YouTube links)

    def _media_box(self, info: media.MediaInfo) -> QFrame:
        box, g = panel(8)
        g.addWidget(caption("YouTube video"))
        title = QLabel(f"<b>{html.escape(info.title)}</b>")
        title.setWordWrap(True)
        mins, secs = divmod(info.duration, 60)
        hours, mins = divmod(mins, 60)
        length = f"{hours}:{mins:02d}:{secs:02d}" if hours else f"{mins}:{secs:02d}"
        meta = QLabel(" · ".join(x for x in (info.uploader, length if info.duration else "") if x))
        meta.setObjectName("muted")
        self.kind_video = QRadioButton("Video")
        self.kind_audio = QRadioButton("Audio only")
        self.kind_video.setEnabled(any(v.kind == "video" for v in info.variants))
        self.kind_audio.setEnabled(any(v.kind == "audio" for v in info.variants))
        self.quality = QComboBox()
        row = QHBoxLayout()
        row.addWidget(self.kind_video)
        row.addWidget(self.kind_audio)
        row.addSpacing(10)
        row.addWidget(form_label("Quality"))
        row.addWidget(self.quality, 1)
        g.addWidget(title)
        g.addWidget(meta)
        g.addLayout(row)
        self.kind_video.toggled.connect(lambda on: on and self._fill_quality())
        self.kind_audio.toggled.connect(lambda on: on and self._fill_quality())
        self.quality.currentIndexChanged.connect(self._variant_changed)
        return box

    def _fill_quality(self, prefer: str = "") -> None:
        kind = "video" if self.kind_video.isChecked() else "audio"
        options = [v for v in self.media_info.variants if v.kind == kind]
        default = self.media_info.default_variant(kind, prefer)
        self.quality.blockSignals(True)
        self.quality.clear()
        for v in options:
            self.quality.addItem(v.label, v)
        self.quality.setCurrentIndex(max(0, options.index(default) if default in options else 0))
        self.quality.blockSignals(False)
        target = "Video" if kind == "video" else "Music"
        if any(c.name == target for c in self.settings.categories):
            self.category.setCurrentText(target)
        self._variant_changed()

    def _variant_changed(self, *_):
        v = self.quality.currentData()
        if v is None:
            return
        self.variant = v
        self.size_label.setText(("~" if v.convert == "mp3" else "") + (fmt.size(v.size) or "(unknown)"))
        root, _ = os.path.splitext(self.save_as.currentText())
        self.save_as.setCurrentText(f"{root}.{v.ext}")
        self._category_changed(self.category.currentText())

    def _category_changed(self, name: str) -> None:
        cat = self.settings.category(name)
        self.pic.setPixmap(icon(category_icon(cat.name)).pixmap(39, 39))
        self.header_icon.setPixmap(icon(category_icon(cat.name)).pixmap(22, 22))
        self.remember.setText(f'Remember this path for "{cat.name}" category')
        current = os.path.basename(self.save_as.currentText()) or self.filename
        path = os.path.join(cat.save_dir, current)
        self.save_as.clear()
        self.save_as.addItem(path)
        for other in self.settings.categories:
            if other.save_dir != cat.save_dir:
                self.save_as.addItem(os.path.join(other.save_dir, current))
        self.save_as.setCurrentIndex(0)

    def _add_category(self) -> None:
        if self.new_category and (name := self.new_category(self)):
            self.category.addItem(name)
            self.category.setCurrentText(name)

    def _browse(self) -> None:
        path, _ = QFileDialog.getSaveFileName(self, "Save As", self.save_as.currentText())
        if path:
            self.save_as.setCurrentText(path)

    def _later(self) -> None:
        btn = self.sender()
        q = queue_menu(self, self.queues, btn.mapToGlobal(btn.rect().bottomLeft()))
        if q:
            self._finish("later", q)

    def _finish(self, action: str, queue: str) -> None:
        path = self.save_as.currentText().strip()
        if not path:
            return
        cat = self.settings.category(self.category.currentText())
        if self.remember.isChecked():
            cat.save_dir = os.path.dirname(path)
        self.result_value = FileInfoResult(action, path, cat.name, self.description.text(), queue, self.variant)
        self.accept()


class DuplicateDialog(QDialog):
    """The "Duplicate download link" dialog."""

    CHOICES = [
        ("number", "Add the duplicate with a numbered file name"),
        ("overwrite", "Add the duplicate and overwrite the existing file"),
        ("complete", "If existing file is complete, show download complete dialog; otherwise resume it"),
    ]

    def __init__(self, url: str, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Duplicate download link")
        self.setMinimumWidth(600)  # height follows from the layout at this width
        body, footer = shell(self, "queue", "Duplicate download link", "This file is already in your download list.")
        edit = link_field(QLineEdit(url))
        edit.setReadOnly(True)
        edit.setFocusPolicy(Qt.NoFocus)
        body.addWidget(edit)
        box, lay = panel()
        self.group = QButtonGroup(self)
        for i, (key, text) in enumerate(self.CHOICES):
            rb = QRadioButton(text)
            rb.setChecked(i == 0)
            self.group.addButton(rb, i)
            lay.addWidget(rb)
        body.addWidget(box)
        self.remember = QCheckBox("Remember my selection and don't ask again (change it in Options → Downloads)")
        body.addWidget(self.remember)
        ok, cancel = primary(QPushButton("OK")), QPushButton("Cancel")
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        footer.addStretch(1)
        footer.addWidget(cancel)
        footer.addWidget(ok)

    def choice(self) -> str:
        return self.CHOICES[self.group.checkedId()][0]


class BatchDialog(QDialog):
    """The "Batch download" dialog: expand one asterisk into a numbered or lettered range."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Batch download")
        self.setMinimumWidth(620)  # height follows from the layout at this width
        body, footer = shell(self, "add-url", "Batch download",
                             "Add a numbered series of files: put one asterisk in the address, e.g. "
                             "https://www.example.com/pictures/img*.jpg")
        self.address = QComboBox()
        self.address.setEditable(True)
        self.address.setCurrentText(clipboard_url())
        link_combo(self.address)
        body.addLayout(form_row("Address", self.address, width=64))

        box, b = panel()
        b.addWidget(caption("Replace asterisk with"))
        self.numbers = QRadioButton("Numbers")
        self.letters = QRadioButton("Letters")
        self.numbers.setChecked(True)
        kinds = QHBoxLayout()
        kinds.addWidget(self.numbers)
        kinds.addSpacing(20)
        kinds.addWidget(self.letters)
        kinds.addStretch(1)
        b.addLayout(kinds)
        self.num_from, self.num_to, self.wildcard_size = QSpinBox(), QSpinBox(), QSpinBox()
        for sb in (self.num_from, self.num_to, self.wildcard_size):
            sb.setButtonSymbols(QSpinBox.NoButtons)
            sb.setFixedSize(80, 30)
            sb.setFont(mono_font(sb.font()))
        for sb in (self.num_from, self.num_to):
            sb.setRange(0, 999999)
        self.num_from.setValue(1)
        self.num_to.setValue(10)
        self.wildcard_size.setRange(1, 9)
        self.let_from, self.let_to = QLineEdit("a"), QLineEdit("z")
        for e in (self.let_from, self.let_to):
            e.setMaxLength(1)
            e.setFixedSize(80, 30)
            e.setFont(mono_font(e.font()))
        rng = QHBoxLayout()
        rng.setSpacing(12)
        rng.addWidget(QLabel("From"))
        rng.addWidget(self.num_from)
        rng.addWidget(self.let_from)
        rng.addSpacing(6)
        rng.addWidget(QLabel("To"))
        rng.addWidget(self.num_to)
        rng.addWidget(self.let_to)
        rng.addSpacing(6)
        rng.addWidget(QLabel("Wildcard size"))
        rng.addWidget(self.wildcard_size)
        rng.addStretch(1)
        b.addLayout(rng)
        body.addWidget(box)
        self.auth = AuthBox(compact=True)
        body.addWidget(self.auth)

        pv, pl = panel(6)
        grid = QGridLayout()
        grid.setHorizontalSpacing(12)
        self.first, self.second, self.last = QLabel(), QLabel(), QLabel()
        for r, (title, lab) in enumerate((("First file", self.first), ("Second file", self.second),
                                          ("Last file", self.last))):
            grid.addWidget(form_label(title), r, 0)
            lab.setObjectName("link")
            lab.setFont(mono_font(lab.font()))
            lab.setTextInteractionFlags(Qt.TextSelectableByMouse)
            grid.addWidget(lab, r, 1)
        grid.setColumnStretch(1, 1)
        pl.addLayout(grid)
        body.addWidget(pv)

        self.count = form_label("")
        ok, cancel = primary(QPushButton("OK")), QPushButton("Cancel")
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        footer.addWidget(self.count)
        footer.addStretch(1)
        footer.addWidget(cancel)
        footer.addWidget(ok)

        for w in (self.num_from, self.num_to, self.wildcard_size):
            w.valueChanged.connect(self._update)
        for w in (self.let_from, self.let_to):
            w.textChanged.connect(self._update)
        self.address.currentTextChanged.connect(self._update)
        self.numbers.toggled.connect(self._update)
        self._update()

    def urls(self) -> list[str]:
        pattern = self.address.currentText().strip()
        if pattern.count("*") != 1:
            return []
        if self.numbers.isChecked():
            a, b, w = self.num_from.value(), self.num_to.value(), self.wildcard_size.value()
            step = 1 if b >= a else -1
            parts = [str(n).zfill(w) for n in range(a, b + step, step)]
        else:
            letters = string.ascii_lowercase + string.ascii_uppercase
            a, b = self.let_from.text() or "a", self.let_to.text() or "z"
            if a not in letters or b not in letters:
                return []
            i, j = letters.index(a), letters.index(b)
            parts = list(letters[min(i, j): max(i, j) + 1])
        return [pattern.replace("*", p) for p in parts]

    def _update(self) -> None:
        self.num_from.setVisible(self.numbers.isChecked())
        self.num_to.setVisible(self.numbers.isChecked())
        self.wildcard_size.setEnabled(self.numbers.isChecked())
        self.let_from.setVisible(self.letters.isChecked())
        self.let_to.setVisible(self.letters.isChecked())
        urls = self.urls()
        self.first.setText(urls[0] if urls else "")
        self.second.setText(urls[1] if len(urls) > 1 else "")
        self.last.setText(urls[-1] if urls else "")
        self.count.setText(f"{len(urls)} files will be added" if urls else "Put one * in the address")


# --------------------------------------------------------------------------- links

class LinkParser(HTMLParser):
    def __init__(self, base: str):
        super().__init__()
        self.base = base
        self.links: list[tuple[str, str, str]] = []  # url, text, kind
        self._href: str | None = None
        self._text: list[str] = []

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "a" and a.get("href"):
            self._href, self._text = urljoin(self.base, a["href"]), []
        elif tag in ("img", "source", "video", "audio", "embed") and a.get("src"):
            self.links.append((urljoin(self.base, a["src"]), a.get("alt", ""), tag))

    def handle_data(self, data):
        if self._href:
            self._text.append(data)

    def handle_endtag(self, tag):
        if tag == "a" and self._href:
            self.links.append((self._href, " ".join("".join(self._text).split()), "a"))
            self._href = None


def extract_links(html: str, base: str) -> list[tuple[str, str, str]]:
    p = LinkParser(base)
    p.feed(html)
    return [(u, t, k) for u, t, k in p.links if urlsplit(u).scheme in ("http", "https", "ftp")]


HTML_EXT = {"", "htm", "html", "php", "asp", "aspx", "jsp", "shtml", "cgi"}
IMAGE_EXT = {"jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "ico"}


@dataclass
class LinkChoice:
    url: str
    filename: str
    description: str
    category: str
    save_dir: str


class LinksDialog(QDialog):
    """The "Download All Links" dialog: pick links from a page or the clipboard."""

    def __init__(self, settings: Settings, links: list[tuple[str, str, str]], parent=None):
        super().__init__(parent)
        self.settings = settings
        self.links = links
        self.setWindowTitle("Download All Links")
        self.resize(840, 640)
        body, footer = shell(self, "grabber", "Download All Links", f"{len(links)} links found · check the ones to add")
        # A check column with a check-all box in its header, like the download list.
        self.tree = QTreeWidget()
        self.header = CheckHeader(self._check_state, self.tree)
        self.header.toggled.connect(lambda: self._check_all(self._check_state() != Qt.Checked))
        self.tree.setHeader(self.header)
        self.tree.setHeaderLabels(["", "FILE", "TYPE", "ADDRESS"])
        self.header.setDefaultAlignment(Qt.AlignLeft | Qt.AlignVCenter)
        self.tree.setRootIsDecorated(False)
        self.tree.setAlternatingRowColors(True)
        self.tree.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.tree.setItemDelegateForColumn(2, TypePillDelegate(self.tree))
        self.header.setSectionResizeMode(0, QHeaderView.Fixed)
        self.header.setSectionResizeMode(3, QHeaderView.Stretch)
        self.tree.setColumnWidth(0, 34)
        self.tree.setColumnWidth(2, 70)
        self.tree.itemChanged.connect(lambda *_: self._count())
        body.addWidget(self.tree, 1)

        row = QHBoxLayout()
        check_all, uncheck_all = QPushButton("Check All"), QPushButton("Uncheck All")
        check_all.clicked.connect(lambda: self._check_all(True))
        uncheck_all.clicked.connect(lambda: self._check_all(False))
        row.addWidget(check_all)
        row.addWidget(uncheck_all)
        row.addStretch(1)
        self.use_text = QCheckBox("Use link texts as download descriptions")
        self.use_text.setChecked(True)
        self.hide_html = QCheckBox("Hide HTML files")
        self.hide_images = QCheckBox("Hide images")
        self.hide_images.setToolTip("Hide images located on this web page")
        self.hide_repeated = QCheckBox("Hide repeated files")
        for c in (self.hide_html, self.hide_repeated):
            c.setChecked(True)
        for c in (self.hide_html, self.hide_images, self.hide_repeated):
            row.addWidget(c)
            c.toggled.connect(self._fill)
        body.addLayout(row)

        box, b = panel()
        b.addWidget(caption("Save to"))
        self.by_category = QRadioButton("Every file to the folder of its category")
        self.one_category = QRadioButton("All files to one category")
        self.one_dir = QRadioButton("All files to one directory")
        self.by_category.setChecked(True)
        self.cat_combo = QComboBox()
        self.cat_combo.addItems([c.name for c in settings.categories])
        self.cat_combo.setMinimumWidth(180)
        self.dir_edit = QLineEdit(settings.categories[0].save_dir)
        browse = QPushButton("Browse...")
        browse.clicked.connect(self._browse)
        b.addWidget(self.by_category)
        r1 = QHBoxLayout()
        r1.addWidget(self.one_category)
        r1.addWidget(self.cat_combo)
        r1.addStretch(1)
        b.addLayout(r1)
        r2 = QHBoxLayout()
        r2.addWidget(self.one_dir)
        r2.addWidget(self.dir_edit, 1)
        r2.addWidget(browse)
        b.addLayout(r2)
        body.addWidget(box)
        body.addWidget(self.use_text)

        self.checked_label = pill_label("", "accent")
        ok, cancel = primary(QPushButton("OK")), QPushButton("Cancel")
        ok.setDefault(True)
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        footer.addWidget(self.checked_label)
        footer.addStretch(1)
        footer.addWidget(cancel)
        footer.addWidget(ok)
        self._fill()

    def _count(self) -> None:
        n = sum(1 for i in range(self.tree.topLevelItemCount()) if self.tree.topLevelItem(i).checkState(0) == Qt.Checked)
        self.checked_label.setText(f"{n} checked")
        self.header.viewport().update()

    def _check_state(self) -> Qt.CheckState:
        states = {self.tree.topLevelItem(i).checkState(0) for i in range(self.tree.topLevelItemCount())}
        if states == {Qt.Checked}:
            return Qt.Checked
        return Qt.PartiallyChecked if Qt.Checked in states else Qt.Unchecked

    def _fill(self) -> None:
        self.tree.clear()
        seen = set()
        mono = mono_font(self.tree.font())
        muted = QColor(theme.colors()["mu"])
        for url, text, kind in self.links:
            name = safe_filename(filename_from_url(url))
            ext = categories.extension(name)
            if self.hide_html.isChecked() and kind == "a" and ext in HTML_EXT:
                continue
            if self.hide_images.isChecked() and kind == "img":
                continue
            if self.hide_repeated.isChecked() and url in seen:
                continue
            seen.add(url)
            item = QTreeWidgetItem(["", name, ext.upper(), url.split("://", 1)[-1]])
            item.setData(0, Qt.UserRole, (url, text, name))
            item.setIcon(1, icon(category_icon(categories.classify(self.settings, name, url).name)))
            item.setToolTip(1, text)
            item.setToolTip(3, url)
            item.setFont(3, mono)
            item.setForeground(3, muted)
            item.setFlags(item.flags() | Qt.ItemIsUserCheckable)
            item.setCheckState(0, Qt.Checked if categories.is_auto_capture_type(self.settings, name) else Qt.Unchecked)
            self.tree.addTopLevelItem(item)
        self.tree.resizeColumnToContents(1)
        self._count()

    def _check_all(self, on: bool) -> None:
        for i in range(self.tree.topLevelItemCount()):
            self.tree.topLevelItem(i).setCheckState(0, Qt.Checked if on else Qt.Unchecked)

    def _browse(self) -> None:
        d = QFileDialog.getExistingDirectory(self, "Save To", self.dir_edit.text())
        if d:
            self.dir_edit.setText(d)
            self.one_dir.setChecked(True)

    def choices(self) -> list[LinkChoice]:
        out = []
        for i in range(self.tree.topLevelItemCount()):
            item = self.tree.topLevelItem(i)
            if item.checkState(0) != Qt.Checked:
                continue
            url, text, name = item.data(0, Qt.UserRole)
            if self.one_category.isChecked():
                cat = self.settings.category(self.cat_combo.currentText())
            else:
                cat = categories.classify(self.settings, name, url)
            save_dir = self.dir_edit.text() if self.one_dir.isChecked() else cat.save_dir
            out.append(LinkChoice(url, name, text if self.use_text.isChecked() else "", cat.name, save_dir))
        return out
