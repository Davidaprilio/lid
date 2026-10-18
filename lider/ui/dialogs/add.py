"""Dialogs for adding downloads: address, file info, duplicate, batch and link list."""

from __future__ import annotations

import html
import os
import re
import string
from dataclasses import dataclass
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit

from PySide6.QtCore import Qt
from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import (
    QAbstractItemView,
    QButtonGroup,
    QCheckBox,
    QComboBox,
    QDialog,
    QFileDialog,
    QGridLayout,
    QGroupBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLayout,
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
from ..icons import file_icon, icon
from ..util import dlu

URL_RE = re.compile(r"^(https?|ftp)://\S+$", re.I)


def clipboard_url() -> str:
    text = QGuiApplication.clipboard().text().strip()
    return text if URL_RE.match(text) else ""


def queue_menu(parent: QWidget, queues: list[str], at) -> str | None:
    menu = QMenu(parent)
    for q in queues:
        menu.addAction(q).setData(q)
    act = menu.exec(at)
    return act.data() if act else None


class AuthBox(QGroupBox):
    """"Use authorization" checkbox-group with Login / Password (dialogs 148 and 280)."""

    def __init__(self, parent=None):
        super().__init__("Use &authorization", parent)
        self.setCheckable(True)
        self.setChecked(False)
        lay = QHBoxLayout(self)
        self.login = QLineEdit()
        self.password = QLineEdit()
        self.password.setEchoMode(QLineEdit.Password)
        lay.addWidget(QLabel("Login"))
        lay.addWidget(self.login)
        lay.addWidget(QLabel("Password"))
        lay.addWidget(self.password)

    def credentials(self) -> tuple[str, str]:
        return (self.login.text(), self.password.text()) if self.isChecked() else ("", "")


class AddUrlDialog(QDialog):
    """The "Enter new address to download" dialog."""

    history: list[str] = []

    def __init__(self, parent=None, url: str = ""):
        super().__init__(parent)
        self.setWindowTitle("Enter new address to download")
        self.resize(dlu(373, 57).width(), 0)
        grid = QGridLayout(self)
        self.address = QComboBox()
        self.address.setEditable(True)
        self.address.addItems(self.history)
        self.address.setCurrentText(url or clipboard_url())
        self.address.lineEdit().selectAll()
        self.auth = AuthBox()
        ok = QPushButton("O&K")
        ok.setDefault(True)
        cancel = QPushButton("&Cancel")
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        self.youtube_hint = QWidget()
        hint = QHBoxLayout(self.youtube_hint)
        hint.setContentsMargins(0, 0, 0, 0)
        pic = QLabel()
        pic.setPixmap(icon("category-video").pixmap(16, 16))
        hint.addWidget(pic)
        hint.addWidget(QLabel("<b>YouTube video detected</b> — choose the resolution or audio only "
                              "in the next window."), 1)
        grid.addWidget(QLabel("Address"), 0, 0)
        grid.addWidget(self.address, 0, 1)
        grid.addWidget(ok, 0, 2)
        grid.addWidget(self.youtube_hint, 1, 1)
        grid.addWidget(self.auth, 2, 0, 1, 2)
        grid.addWidget(cancel, 1, 2, Qt.AlignTop)
        grid.setColumnStretch(1, 1)
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
                 media_info: media.MediaInfo | None = None, media_kind: str = "video", media_quality: str = ""):
        super().__init__(parent)
        self.media_info = media_info
        self.variant: media.Variant | None = None
        self.settings = settings
        self.queues = queues
        self.filename = filename
        self.new_category = new_category
        self.result_value: FileInfoResult | None = None
        self.setWindowTitle("Download File Info")
        self.resize(dlu(380, 128))
        self.setMaximumWidth(1000)

        grid = QGridLayout()
        self.url = QLineEdit(url)
        self.category = QComboBox()
        self.category.addItems([c.name for c in settings.categories])
        add_cat = QPushButton("+")
        add_cat.setFixedWidth(28)
        add_cat.setToolTip("Add category")
        add_cat.clicked.connect(self._add_category)
        self.save_as = QComboBox()
        self.save_as.setEditable(True)
        self.save_as.setSizeAdjustPolicy(QComboBox.AdjustToMinimumContentsLengthWithIcon)
        self.save_as.setMinimumContentsLength(30)
        self.category.setMinimumWidth(dlu(116, 0).width())
        browse = QPushButton("...")
        browse.setFixedWidth(28)
        browse.clicked.connect(self._browse)
        self.remember = QCheckBox()
        self.description = QLineEdit(description)

        cat_row = QHBoxLayout()
        cat_row.addWidget(self.category)
        cat_row.addWidget(add_cat)
        cat_row.addStretch(1)
        save_row = QHBoxLayout()
        save_row.addWidget(self.save_as, 1)
        save_row.addWidget(browse)

        grid.addWidget(QLabel("URL"), 0, 0)
        grid.addWidget(self.url, 0, 1)
        grid.addWidget(QLabel("Category"), 1, 0)
        grid.addLayout(cat_row, 1, 1)
        grid.addWidget(QLabel("Save As"), 2, 0)
        grid.addLayout(save_row, 2, 1)
        grid.addWidget(self.remember, 3, 1)
        if media_info:
            self.url.setReadOnly(True)
            grid.addWidget(self._media_box(media_info), 4, 0, 1, 2)
        grid.addWidget(QLabel("Description"), 5, 0)
        grid.addWidget(self.description, 5, 1)
        grid.setColumnStretch(1, 1)

        side = QVBoxLayout()
        self.pic = pic = QLabel()
        pic.setPixmap(file_icon(filename).pixmap(32, 32))
        pic.setAlignment(Qt.AlignCenter)
        self.size_label = QLabel(fmt.size(size) or "(unknown)")
        self.size_label.setAlignment(Qt.AlignCenter)
        side.addSpacing(20)
        side.addWidget(pic)
        side.addWidget(self.size_label)
        side.addStretch(1)

        top = QHBoxLayout()
        top.addLayout(grid, 1)
        top.addLayout(side)

        later = QPushButton("Download &Later")
        start = QPushButton("&Start Download")
        cancel = QPushButton("&Cancel")
        start.setDefault(True)
        later.clicked.connect(self._later)
        start.clicked.connect(lambda: self._finish("start", ""))
        cancel.clicked.connect(self.reject)
        buttons = QHBoxLayout()
        buttons.addStretch(1)
        for b in (later, start, cancel):
            b.setMinimumWidth(dlu(74, 14).width())
            buttons.addWidget(b)
        buttons.addStretch(1)

        lay = QVBoxLayout(self)
        lay.addLayout(top)
        lay.addLayout(buttons)

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
    def _media_box(self, info: media.MediaInfo) -> QGroupBox:
        box = QGroupBox("YouTube video")
        g = QGridLayout(box)
        title = QLabel(f"<b>{html.escape(info.title)}</b>")
        title.setWordWrap(True)
        mins, secs = divmod(info.duration, 60)
        hours, mins = divmod(mins, 60)
        length = f"{hours}:{mins:02d}:{secs:02d}" if hours else f"{mins}:{secs:02d}"
        meta = QLabel(" · ".join(x for x in (info.uploader, length if info.duration else "") if x))
        meta.setEnabled(False)
        self.kind_video = QRadioButton("Video")
        self.kind_audio = QRadioButton("Audio only")
        self.kind_video.setEnabled(any(v.kind == "video" for v in info.variants))
        self.kind_audio.setEnabled(any(v.kind == "audio" for v in info.variants))
        kinds = QHBoxLayout()
        kinds.addWidget(self.kind_video)
        kinds.addWidget(self.kind_audio)
        kinds.addStretch(1)
        self.quality = QComboBox()
        g.addWidget(title, 0, 0, 1, 2)
        g.addWidget(meta, 1, 0, 1, 2)
        g.addWidget(QLabel("Download"), 2, 0)
        g.addLayout(kinds, 2, 1)
        g.addWidget(QLabel("Quality"), 3, 0)
        g.addWidget(self.quality, 3, 1)
        g.setColumnStretch(1, 1)
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
        self.pic.setPixmap(file_icon(f"x.{v.ext}").pixmap(32, 32))
        root, _ = os.path.splitext(self.save_as.currentText())
        self.save_as.setCurrentText(f"{root}.{v.ext}")
        self._category_changed(self.category.currentText())

    def _category_changed(self, name: str) -> None:
        cat = self.settings.category(name)
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
        self.resize(dlu(286, 154))
        lay = QVBoxLayout(self)
        lay.setSizeConstraint(QLayout.SetFixedSize)
        edit = QLineEdit(url)
        edit.setReadOnly(True)
        lay.addWidget(edit)
        note = QLabel("This file already exists in your download list. You may choose to add a duplicate "
                      "download with a numbered file name, overwrite the existing file, or show the existing one.")
        note.setWordWrap(True)
        lay.addWidget(note)
        self.group = QButtonGroup(self)
        for i, (key, text) in enumerate(self.CHOICES):
            rb = QRadioButton(text)
            rb.setChecked(i == 0)
            self.group.addButton(rb, i)
            lay.addWidget(rb)
        row = QHBoxLayout()
        ok, cancel = QPushButton("OK"), QPushButton("Cancel")
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        row.addStretch(1)
        row.addWidget(ok)
        row.addWidget(cancel)
        row.addStretch(1)
        lay.addLayout(row)
        self.remember = QCheckBox("Remember my selection and don't show this dialog again.\n"
                                  "You can change it later in Options->Downloads.")
        lay.addWidget(self.remember)

    def choice(self) -> str:
        return self.CHOICES[self.group.checkedId()][0]


class BatchDialog(QDialog):
    """The "Batch download" dialog: expand one asterisk into a numbered or lettered range."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Batch download")
        self.resize(dlu(316, 238))
        lay = QVBoxLayout(self)
        lay.setSizeConstraint(QLayout.SetFixedSize)
        intro = QLabel("It's possible to add a group of sequential file names like img001.jpg, img002.jpg, etc., "
                       "img100.jpg to the download queue. Use the asterisk wildcard for the file name pattern.\n"
                       "For example:   http://www.example.com/pictures/img*.jpg")
        intro.setWordWrap(True)
        lay.addWidget(intro)
        row = QHBoxLayout()
        row.addWidget(QLabel("Address"))
        self.address = QComboBox()
        self.address.setEditable(True)
        self.address.setCurrentText(clipboard_url())
        row.addWidget(self.address, 1)
        lay.addLayout(row)

        box = QGroupBox("Replace asterisk to")
        g = QGridLayout(box)
        self.numbers = QRadioButton("Numbers")
        self.letters = QRadioButton("Letters")
        self.numbers.setChecked(True)
        self.num_from, self.num_to, self.width = QSpinBox(), QSpinBox(), QSpinBox()
        for sb in (self.num_from, self.num_to):
            sb.setRange(0, 999999)
        self.num_from.setValue(1)
        self.num_to.setValue(10)
        self.width.setRange(1, 9)
        self.let_from, self.let_to = QLineEdit("a"), QLineEdit("z")
        for e in (self.let_from, self.let_to):
            e.setMaxLength(1)
            e.setFixedWidth(40)
        g.addWidget(self.numbers, 0, 0)
        g.addWidget(self.letters, 0, 2)
        g.addWidget(QLabel("From:"), 1, 0)
        g.addWidget(self.num_from, 1, 1)
        g.addWidget(self.let_from, 1, 1)
        g.addWidget(QLabel("To:"), 1, 2)
        g.addWidget(self.num_to, 1, 3)
        g.addWidget(self.let_to, 1, 3)
        g.addWidget(QLabel("Wildcard size"), 1, 4)
        g.addWidget(self.width, 1, 5)
        lay.addWidget(box)
        self.auth = AuthBox()
        lay.addWidget(self.auth)

        preview = QGridLayout()
        self.first, self.second, self.last = QLabel(), QLabel(), QLabel()
        for r, (title, lab) in enumerate((("First file:", self.first), ("Second file:", self.second),
                                          ("...", None), ("Last file:", self.last))):
            preview.addWidget(QLabel(title), r, 0)
            if lab:
                lab.setTextInteractionFlags(Qt.TextSelectableByMouse)
                preview.addWidget(lab, r, 1)
        preview.setColumnStretch(1, 1)
        lay.addLayout(preview)
        buttons = QHBoxLayout()
        buttons.addStretch(1)
        ok, cancel = QPushButton("OK"), QPushButton("Cancel")
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        buttons.addWidget(ok)
        buttons.addWidget(cancel)
        lay.addLayout(buttons)

        for w in (self.num_from, self.num_to, self.width):
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
            a, b, w = self.num_from.value(), self.num_to.value(), self.width.value()
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
        self.width.setEnabled(self.numbers.isChecked())
        self.let_from.setVisible(self.letters.isChecked())
        self.let_to.setVisible(self.letters.isChecked())
        urls = self.urls()
        self.first.setText(urls[0] if urls else "")
        self.second.setText(urls[1] if len(urls) > 1 else "")
        self.last.setText(urls[-1] if urls else "")


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
        self.resize(dlu(532, 335))
        lay = QVBoxLayout(self)
        lay.addWidget(QLabel("Please check the links, which you want to add to the download list of LID"))
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["File Name", "Description", "Address"])
        self.tree.setRootIsDecorated(False)
        self.tree.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.tree.header().setSectionResizeMode(2, QHeaderView.Stretch)
        lay.addWidget(self.tree, 1)

        bottom = QHBoxLayout()
        box = QGroupBox("Save To")
        b = QGridLayout(box)
        self.by_category = QRadioButton("Every file to the directory according to the category of the file")
        self.one_category = QRadioButton("All files to one category")
        self.one_dir = QRadioButton("All files to one directory")
        self.by_category.setChecked(True)
        self.cat_combo = QComboBox()
        self.cat_combo.addItems([c.name for c in settings.categories])
        self.dir_edit = QLineEdit(settings.categories[0].save_dir)
        browse = QPushButton("Browse...")
        browse.clicked.connect(self._browse)
        b.addWidget(self.by_category, 0, 0, 1, 2)
        b.addWidget(self.one_category, 1, 0, 1, 2)
        b.addWidget(self.cat_combo, 2, 0)
        b.addWidget(self.one_dir, 3, 0, 1, 2)
        b.addWidget(self.dir_edit, 4, 0)
        b.addWidget(browse, 4, 1)
        bottom.addWidget(box, 1)

        right = QVBoxLayout()
        row = QHBoxLayout()
        check_all, uncheck_all = QPushButton("Check All"), QPushButton("Uncheck All")
        check_all.clicked.connect(lambda: self._check_all(True))
        uncheck_all.clicked.connect(lambda: self._check_all(False))
        row.addWidget(check_all)
        row.addWidget(uncheck_all)
        row.addStretch(1)
        right.addLayout(row)
        self.use_text = QCheckBox("Use link texts as download descriptions")
        self.use_text.setChecked(True)
        self.hide_html = QCheckBox("Hide HTML files")
        self.hide_images = QCheckBox("Hide images located on this web page")
        self.hide_repeated = QCheckBox("Hide repeated files")
        for c in (self.hide_html, self.hide_repeated):
            c.setChecked(True)
        for c in (self.use_text, self.hide_html, self.hide_images, self.hide_repeated):
            right.addWidget(c)
            if c is not self.use_text:
                c.toggled.connect(self._fill)
        right.addStretch(1)
        ok_row = QHBoxLayout()
        ok_row.addStretch(1)
        ok, cancel = QPushButton("OK"), QPushButton("Cancel")
        ok.clicked.connect(self.accept)
        cancel.clicked.connect(self.reject)
        ok_row.addWidget(ok)
        ok_row.addWidget(cancel)
        right.addLayout(ok_row)
        bottom.addLayout(right, 1)
        lay.addLayout(bottom)
        self._fill()

    def _fill(self) -> None:
        self.tree.clear()
        seen = set()
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
            item = QTreeWidgetItem([name, text, url])
            item.setIcon(0, file_icon(name))
            item.setFlags(item.flags() | Qt.ItemIsUserCheckable)
            item.setCheckState(0, Qt.Checked if categories.is_auto_capture_type(self.settings, name) else Qt.Unchecked)
            self.tree.addTopLevelItem(item)
        self.tree.resizeColumnToContents(0)

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
            name, text, url = item.text(0), item.text(1), item.text(2)
            if self.one_category.isChecked():
                cat = self.settings.category(self.cat_combo.currentText())
            else:
                cat = categories.classify(self.settings, name, url)
            save_dir = self.dir_edit.text() if self.one_dir.isChecked() else cat.save_dir
            out.append(LinkChoice(url, name, text if self.use_text.isChecked() else "", cat.name, save_dir))
        return out
