"""Widgets and delegates of the Ember look: tag input, check-all header, status pills, tree badges."""

from __future__ import annotations

from typing import Callable

from PySide6.QtCore import QEvent, QPoint, QPointF, QRect, QRectF, QSize, Qt, Signal
from PySide6.QtGui import QColor, QFont, QFontDatabase, QFontMetrics, QIcon, QPainter, QPainterPath, QPalette, QPen, QPixmap
from PySide6.QtWidgets import (
    QBoxLayout,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLayout,
    QLineEdit,
    QSizePolicy,
    QStyle,
    QStyledItemDelegate,
    QStyleOptionToolButton,
    QStyleOptionViewItem,
    QStylePainter,
    QToolButton,
    QVBoxLayout,
    QWidget,
    QWidgetItem,
)

from . import theme
from .icons import bind, icon

STATUS_ROLE = Qt.UserRole + 10  # (kind, progress 0..1, text); kind: run | pause | queue | done | error | ""
COUNT_ROLE = Qt.UserRole + 11  # number shown as a badge in the Categories tree


def mono_font(base: QFont | None = None) -> QFont:
    f = QFontDatabase.systemFont(QFontDatabase.FixedFont)
    if base is not None:
        f.setPointSizeF(base.pointSizeF() * 0.95)
    return f


def _c(name: str) -> QColor:
    return QColor(theme.colors()[name])


def _alpha(col: QColor, a: int) -> QColor:
    col = QColor(col)
    col.setAlpha(a)
    return col


# ---------------------------------------------------------------- dialog header
def dialog_header(icon_name: str, title: str, subtitle: str = "", tile: bool = False) -> QWidget:
    """Icon (bare, or 22 px on a 40 px tile), bold title and a muted line under it; sits at the top
    of a dialog. The icon label is ``header.pic``."""
    w = QWidget()
    lay = QHBoxLayout(w)
    lay.setContentsMargins(0, 0, 0, 2)
    lay.setSpacing(12)
    pic = QLabel()
    pic.setAlignment(Qt.AlignCenter)
    if tile:
        box = QFrame()
        box.setObjectName("tileSmall")
        box.setFixedSize(40, 40)
        bl = QVBoxLayout(box)
        bl.setContentsMargins(0, 0, 0, 0)
        bl.addWidget(pic)
        pic.setPixmap(icon(icon_name).pixmap(22, 22))
        lay.addWidget(box, 0, Qt.AlignTop)
    else:
        pic.setPixmap(icon(icon_name).pixmap(30, 30))
        lay.addWidget(pic, 0, Qt.AlignTop)
    w.pic = pic
    text = QVBoxLayout()
    text.setSpacing(2)
    t = QLabel(title)
    t.setObjectName("dialogTitle")
    f = QFont(t.font())
    f.setPointSizeF(f.pointSizeF() * 1.25)
    f.setBold(True)
    t.setFont(f)
    text.addWidget(t)
    if subtitle:
        s = QLabel(subtitle)
        s.setObjectName("muted")
        s.setWordWrap(True)
        text.addWidget(s)
    lay.addLayout(text, 1)
    return w


# ---------------------------------------------------------------- dialog building blocks
def shell(dialog: QWidget, icon_name: str | None = None, title: str = "", subtitle: str = "",
          spacing: int = 10, tile: bool = False) -> tuple[QVBoxLayout, QHBoxLayout]:
    """The frame of every dialog: optional header, padded body and a footer bar. Returns (body, footer);
    the header's icon label is ``dialog.header_icon``."""
    outer = QVBoxLayout(dialog)
    outer.setContentsMargins(0, 0, 0, 0)
    outer.setSpacing(0)
    content = QWidget()
    body = QVBoxLayout(content)
    body.setContentsMargins(18, 16, 18, 14)
    body.setSpacing(spacing)
    if icon_name:
        header = dialog_header(icon_name, title, subtitle, tile)
        dialog.header_icon = header.pic
        body.addWidget(header)
    outer.addWidget(content, 1)
    bar = QFrame()
    bar.setObjectName("footer")
    footer = QHBoxLayout(bar)
    footer.setContentsMargins(16, 12, 16, 12)
    footer.setSpacing(8)
    outer.addWidget(bar)
    return body, footer


def panel(spacing: int = 12) -> tuple[QFrame, QVBoxLayout]:
    """A rounded card that groups related controls."""
    frame = QFrame()
    frame.setObjectName("panel")
    lay = QVBoxLayout(frame)
    lay.setContentsMargins(14, 14, 14, 14)
    lay.setSpacing(spacing)
    return frame, lay


def caption(text: str) -> QLabel:
    lab = QLabel(text.upper())
    lab.setObjectName("sectionTitle")
    f = QFont(lab.font())
    f.setPointSizeF(f.pointSizeF() * 11 / 12)
    lab.setFont(f)
    return lab


def link_field(edit: QLineEdit) -> QLineEdit:
    """An address field: monospaced, with a link icon in front."""
    edit.setFont(mono_font(edit.font()))
    bind(edit.addAction(icon("link"), QLineEdit.LeadingPosition), "link")
    return edit


def form_label(text: str, width: int = 0) -> QLabel:
    lab = QLabel(text)
    lab.setObjectName("formLabel")
    if width:
        lab.setFixedWidth(width)
    return lab


def form_row(label: str, *items, width: int = 96, stretch_last: bool = True) -> QHBoxLayout:
    """Muted label in a fixed column, then the controls."""
    row = QHBoxLayout()
    row.setSpacing(8)
    row.addWidget(form_label(label, width))
    for i, item in enumerate(items):
        grow = 1 if (stretch_last and i == len(items) - 1) or getattr(item, "_grow", False) else 0
        if isinstance(item, QLayout):
            row.addLayout(item, grow)
        else:
            row.addWidget(item, grow)
    return row


def note(text: str, boxed: bool = False) -> QLabel:
    lab = QLabel(text)
    lab.setWordWrap(True)
    lab.setObjectName("noteBox" if boxed else "faint")
    return lab


def pill_label(text: str, kind: str = "muted") -> QLabel:
    """A small rounded status label; kind is ok, bad, accent or muted."""
    lab = QLabel(text)
    lab.setObjectName({"ok": "pillOk", "bad": "pillBad", "accent": "pillAccent"}.get(kind, "pillMuted"))
    f = QFont(lab.font())
    f.setPointSizeF(f.pointSizeF() * 11.5 / 12)
    f.setWeight(QFont.DemiBold)
    lab.setFont(f)
    return lab


def primary(button):
    """Mark the button that finishes a dialog; the theme paints it in the accent colour."""
    button.setProperty("primary", True)
    return button


def add_header(dialog: QWidget, icon_name: str, title: str, subtitle: str = "") -> None:
    """Put a dialog_header above whatever the dialog's layout already holds."""
    lay = dialog.layout()
    header = dialog_header(icon_name, title, subtitle)
    if isinstance(lay, QGridLayout):
        items = []
        while lay.count():
            pos = lay.getItemPosition(0)
            items.append((lay.takeAt(0), pos))
        cols = max((c + cs for _i, (_r, c, _rs, cs) in items), default=1)
        lay.addWidget(header, 0, 0, 1, cols)
        for item, (r, c, rs, cs) in items:
            lay.addItem(item, r + 1, c, rs, cs)
    elif isinstance(lay, QBoxLayout) and lay.direction() in (QBoxLayout.TopToBottom, QBoxLayout.Down):
        lay.insertWidget(0, header)


# ---------------------------------------------------------------- tag input
class FlowLayout(QLayout):
    """Lays children left to right and wraps them onto new lines."""

    def __init__(self, parent=None, spacing: int = 6):
        super().__init__(parent)
        self._items: list = []
        self._spacing = spacing
        self.setContentsMargins(8, 8, 8, 8)

    def addItem(self, item):
        self._items.append(item)

    def insertWidget(self, index: int, w: QWidget) -> None:
        self.addChildWidget(w)
        self._items.insert(index, QWidgetItem(w))
        self.invalidate()

    def count(self):
        return len(self._items)

    def itemAt(self, i):
        return self._items[i] if 0 <= i < len(self._items) else None

    def takeAt(self, i):
        return self._items.pop(i) if 0 <= i < len(self._items) else None

    def expandingDirections(self):
        return Qt.Orientations(0)

    def hasHeightForWidth(self):
        return True

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._fit()

    def _fit(self) -> None:
        h = self.flow.heightForWidth(max(1, self.width()))
        if self.minimumHeight() != h:
            self.setMinimumHeight(h)

    def heightForWidth(self, width):
        return self._do_layout(QRect(0, 0, width, 0), True)

    def setGeometry(self, rect):
        super().setGeometry(rect)
        self._do_layout(rect, False)

    def sizeHint(self):
        return self.minimumSize()

    def minimumSize(self):
        s = QSize()
        for it in self._items:
            s = s.expandedTo(it.minimumSize())
        m = self.contentsMargins()
        return s + QSize(m.left() + m.right(), m.top() + m.bottom())

    def _do_layout(self, rect: QRect, test: bool) -> int:
        m = self.contentsMargins()
        r = rect.adjusted(m.left(), m.top(), -m.right(), -m.bottom())
        x, y, line_h = r.x(), r.y(), 0
        for it in self._items:
            hint = it.sizeHint()
            if x + hint.width() > r.right() + 1 and line_h > 0:
                x, y, line_h = r.x(), y + line_h + self._spacing, 0
            if not test:
                it.setGeometry(QRect(QPoint(x, y), hint))
            x += hint.width() + self._spacing
            line_h = max(line_h, hint.height())
        return y + line_h - rect.y() + m.bottom()


class _TagEdit(QLineEdit):
    backspace_on_empty = Signal()
    commit = Signal()

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key_Return, Qt.Key_Enter) or e.text() == ",":
            self.commit.emit()
            return
        if e.key() == Qt.Key_Backspace and not self.text():
            self.backspace_on_empty.emit()
            return
        super().keyPressEvent(e)


class TagInput(QFrame):
    """Values shown as removable badges. Type and press Enter or comma to add; Backspace removes the last."""

    changed = Signal()

    def __init__(self, values: list[str] | str = (), placeholder: str = "", normalize: Callable[[str], str] | None = None,
                 noun: str = "items", parent=None):
        super().__init__(parent)
        self.setObjectName("tagInput")
        self._normalize = normalize or (lambda s: s.strip())
        self._noun = noun
        self._values: list[str] = []
        self.flow = FlowLayout(self)
        self.edit = _TagEdit()
        self.edit.setPlaceholderText(placeholder)
        self.edit.setMinimumWidth(max(120, self.edit.fontMetrics().horizontalAdvance(placeholder) + 24))
        self.edit.commit.connect(self._commit)
        self.edit.backspace_on_empty.connect(self._remove_last)
        self.edit.installEventFilter(self)
        self.flow.addWidget(self.edit)
        self.setCursor(Qt.IBeamCursor)
        sp = QSizePolicy(QSizePolicy.Preferred, QSizePolicy.Preferred)
        sp.setHeightForWidth(True)
        self.setSizePolicy(sp)
        self.set_values(values)

    # -- public API
    def values(self) -> list[str]:
        self._commit()
        return list(self._values)

    def text(self) -> str:
        return " ".join(self.values())

    def set_values(self, values) -> None:
        if isinstance(values, str):
            values = values.split()
        for i in reversed(range(self.flow.count() - 1)):
            item = self.flow.takeAt(i)
            item.widget().deleteLater()
        self._values = []
        for v in values:
            self._add(v, emit=False)
        self.changed.emit()

    def setText(self, text: str) -> None:
        self.set_values(text)

    def meta_row(self) -> QWidget:
        """'N items · Clear all' line to place under the input."""
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        hint = QLabel("Enter or , adds · Backspace removes the last")
        hint.setObjectName("faint")
        lay.addWidget(hint)
        lay.addStretch(1)
        count = QLabel()
        count.setObjectName("faint")
        clear = QToolButton()
        clear.setText("Clear all")
        clear.setObjectName("ghost")
        clear.clicked.connect(lambda: self.set_values([]))
        lay.addWidget(count)
        lay.addWidget(clear)

        def update():
            count.setText(f"{len(self._values)} {self._noun}")
        self.changed.connect(update)
        update()
        return w

    # -- internals
    def hasHeightForWidth(self):
        return True

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._fit()

    def _fit(self) -> None:
        h = self.flow.heightForWidth(max(1, self.width()))
        if self.minimumHeight() != h:
            self.setMinimumHeight(h)

    def heightForWidth(self, w):
        return self.flow.heightForWidth(w)

    def sizeHint(self):
        return QSize(300, self.flow.heightForWidth(max(300, self.width())))

    def mousePressEvent(self, e):
        self.edit.setFocus()
        super().mousePressEvent(e)

    def eventFilter(self, obj, e):
        if obj is self.edit and e.type() in (QEvent.FocusIn, QEvent.FocusOut):
            self.setProperty("focused", e.type() == QEvent.FocusIn)
            self.style().unpolish(self)
            self.style().polish(self)
            if e.type() == QEvent.FocusOut:
                self._commit()
        return super().eventFilter(obj, e)

    def _commit(self) -> None:
        text = self.edit.text()
        if not text.strip():
            self.edit.clear()
            return
        for part in text.replace(",", " ").split():
            self._add(part)
        self.edit.clear()

    def _add(self, raw: str, emit: bool = True) -> None:
        v = self._normalize(raw)
        if not v or v in self._values:
            return
        self._values.append(v)
        chip = QToolButton()
        chip.setObjectName("tag")
        chip.setText(v)
        chip.setIcon(icon("close"))
        chip.setIconSize(QSize(11, 11))
        chip.setToolButtonStyle(Qt.ToolButtonTextBesideIcon)
        chip.setLayoutDirection(Qt.RightToLeft)
        chip.setCursor(Qt.PointingHandCursor)
        chip.setToolTip(f"Remove {v}")
        chip.clicked.connect(lambda _=False, c=chip: self._remove(c))
        self.flow.insertWidget(self.flow.count() - 1, chip)
        self._fit()
        if emit:
            self.changed.emit()

    def _remove(self, chip: QToolButton) -> None:
        v = chip.text()
        if v in self._values:
            self._values.remove(v)
        for i in range(self.flow.count()):
            if self.flow.itemAt(i).widget() is chip:
                self.flow.takeAt(i)
                break
        chip.deleteLater()
        self.flow.invalidate()
        self._fit()
        self.changed.emit()

    def _remove_last(self) -> None:
        if self.flow.count() > 1:
            self._remove(self.flow.itemAt(self.flow.count() - 2).widget())


def extension_tag(s: str) -> str:
    return s.strip().lstrip(".").lower()


def site_tag(s: str) -> str:
    return s.strip().lower()


# ---------------------------------------------------------------- painting helpers
def _box(p: QPainter, r: QRectF, state: Qt.CheckState) -> None:
    p.save()
    p.setRenderHint(QPainter.Antialiasing)
    path = QPainterPath()
    path.addRoundedRect(r, 4, 4)
    if state == Qt.Unchecked:
        p.fillPath(path, _c("input"))
        p.setPen(QPen(_c("line2"), 1.5))
        p.drawPath(path)
    else:
        p.fillPath(path, _c("ac"))
        p.setPen(QPen(QColor("#FFFFFF"), 2, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        x, y, s = r.x(), r.y(), r.width()
        if state == Qt.Checked:
            p.drawPolyline([QPoint(int(x + s * .27), int(y + s * .52)), QPoint(int(x + s * .43), int(y + s * .68)),
                            QPoint(int(x + s * .74), int(y + s * .33))])
        else:
            p.drawLine(QPoint(int(x + s * .3), int(y + s * .5)), QPoint(int(x + s * .7), int(y + s * .5)))
    p.restore()


def pill(p: QPainter, rect: QRect, text: str, fg: QColor, bg: QColor, font: QFont, fill: float | None = None,
         fill_color: QColor | None = None, width: int | None = None) -> None:
    """A rounded label; with ``fill`` it becomes a progress pill filled from the left."""
    fm = QFontMetrics(font)
    h = min(rect.height() - 8, fm.height() + 8)
    w = width or fm.horizontalAdvance(text) + 20
    w = min(w, rect.width() - 8)
    r = QRectF(rect.x() + 4, rect.y() + (rect.height() - h) / 2, w, h)
    p.save()
    p.setRenderHint(QPainter.Antialiasing)
    path = QPainterPath()
    path.addRoundedRect(r, h / 2, h / 2)
    p.fillPath(path, bg)
    if fill is not None and fill_color is not None:
        p.setClipPath(path)
        p.fillRect(QRectF(r.x(), r.y(), r.width() * max(0.0, min(1.0, fill)), r.height()), _alpha(fill_color, 70))
        p.fillRect(QRectF(r.x(), r.y(), 3, r.height()), fill_color)
        p.setClipping(False)
    p.setFont(font)
    p.setPen(fg)
    p.drawText(r.adjusted(10, 0, -6, 0), Qt.AlignVCenter | Qt.AlignLeft, text)
    p.restore()


# ---------------------------------------------------------------- toolbar chip
def check_box_pixmap(state: Qt.CheckState, size: int, ratio: float = 1.0) -> QPixmap:
    pm = QPixmap(round(size * ratio), round(size * ratio))
    pm.fill(Qt.transparent)
    pm.setDevicePixelRatio(ratio)
    p = QPainter(pm)
    _box(p, QRectF(0.5, 0.5, size - 1, size - 1), state)
    p.end()
    return pm


class ToolButton(QToolButton):
    """A toolbar button measured like the design: padding, icon, gap, text, then a chevron when
    it has a menu. The style sheet still paints the panel; the label is painted here because
    QToolButton keeps a fixed 2 px between icon and text and, inside a QToolBar, draws the
    toolbar's icon size instead of its own."""

    CHEVRON = 11

    def __init__(self, pad: int = 13, gap: int = 7, parent=None):
        super().__init__(parent)
        self._pad, self._gap = pad, gap

    def _parts(self) -> tuple[int, str, bool]:
        icon_w = 0 if self.icon().isNull() else self.iconSize().width()
        text = "" if self.toolButtonStyle() == Qt.ToolButtonIconOnly else self.text()
        return icon_w, text, self.popupMode() == QToolButton.MenuButtonPopup and self.menu() is not None

    def sizeHint(self) -> QSize:
        icon_w, text, menu = self._parts()
        w = 2 * (self._pad + 1) + icon_w
        if text:
            w += (self._gap if icon_w else 0) + self.fontMetrics().horizontalAdvance(text)
        if menu:
            w += self._gap + self.CHEVRON
        return QSize(w, super().sizeHint().height())

    def minimumSizeHint(self) -> QSize:
        return self.sizeHint()

    def paintEvent(self, e) -> None:
        p = QStylePainter(self)
        opt = QStyleOptionToolButton()
        self.initStyleOption(opt)
        opt.text, opt.icon = "", QIcon()
        p.drawComplexControl(QStyle.CC_ToolButton, opt)
        icon_w, text, menu = self._parts()
        x, h = 1 + self._pad, self.height()
        if icon_w:
            size = self.iconSize()
            mode = QIcon.Normal if self.isEnabled() else QIcon.Disabled
            pm = self.icon().pixmap(size, self.devicePixelRatioF(), mode)
            p.drawPixmap(x, (h - size.height()) // 2, pm)
            x += icon_w + (self._gap if text else 0)
        if text:
            group = QPalette.Active if self.isEnabled() else QPalette.Disabled
            p.setPen(self.palette().color(group, self.foregroundRole()))
            tw = self.fontMetrics().horizontalAdvance(text)
            p.drawText(QRect(x, 0, tw + 1, h), Qt.AlignLeft | Qt.AlignVCenter, text)
            x += tw
        if menu:
            p.setRenderHint(QPainter.Antialiasing)
            p.setPen(QPen(_c("fa"), 1.2, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
            k, top = self.CHEVRON / 24, (h - self.CHEVRON) / 2
            left = x + self._gap
            path = QPainterPath()
            path.moveTo(left + 6 * k, top + 9 * k)
            path.lineTo(left + 12 * k, top + 15 * k)
            path.lineTo(left + 18 * k, top + 9 * k)
            p.drawPath(path)


def cross_pixmap(size: int, color: QColor, ratio: float = 1.0) -> QPixmap:
    pm = QPixmap(round(size * ratio), round(size * ratio))
    pm.setDevicePixelRatio(ratio)
    pm.fill(Qt.transparent)
    p = QPainter(pm)
    p.setRenderHint(QPainter.Antialiasing)
    k = size / 24
    p.setPen(QPen(color, 2.4 * k, Qt.SolidLine, Qt.RoundCap))
    p.drawLine(QPointF(6.5 * k, 6.5 * k), QPointF(17.5 * k, 17.5 * k))
    p.drawLine(QPointF(17.5 * k, 6.5 * k), QPointF(6.5 * k, 17.5 * k))
    p.end()
    return pm


class CheckChip(QFrame):
    """'3 checked ✕' in the toolbar: how many rows the toolbar acts on; ✕ clears the checks."""

    cleared = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("chip")
        self.setFixedHeight(30)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(10, 0, 6, 0)
        lay.setSpacing(8)
        self.box = QLabel()
        self.label = QLabel()
        self.close = QToolButton()
        self.close.setObjectName("chipClose")
        self.close.setFixedSize(20, 20)
        self.close.setIconSize(QSize(12, 12))
        self.close.setAutoRaise(True)  # so the white "active" cross shows on hover
        self.close.setToolTip("Clear the checks")
        self.close.setCursor(Qt.PointingHandCursor)
        self.close.clicked.connect(self.cleared.emit)
        lay.addWidget(self.box)
        lay.addWidget(self.label)
        lay.addWidget(self.close)

    def setText(self, text: str) -> None:
        self.label.setText(text)
        self._paint()

    def _paint(self) -> None:
        ratio = self.devicePixelRatioF()
        self.box.setPixmap(check_box_pixmap(Qt.PartiallyChecked, 14, ratio))
        cross = QIcon(cross_pixmap(12, _c("actx"), ratio))
        cross.addPixmap(cross_pixmap(12, QColor("#FFFFFF"), ratio), QIcon.Active)
        self.close.setIcon(cross)

    def changeEvent(self, e) -> None:
        super().changeEvent(e)
        if e.type() == QEvent.StyleChange:  # theme switch
            self._paint()

    def text(self) -> str:
        return self.label.text()


# ---------------------------------------------------------------- download list
class CheckHeader(QHeaderView):
    """Header whose first section is a tri-state check-all box."""

    toggled = Signal()

    def __init__(self, state: Callable[[], Qt.CheckState], parent=None):
        super().__init__(Qt.Horizontal, parent)
        self._state = state

    def paintSection(self, painter, rect, index):
        super().paintSection(painter, rect, index)
        if self.logicalIndex(0) == index and self.visualIndex(index) == 0 and index == 0:
            s = 16
            _box(painter, QRectF(rect.x() + (rect.width() - s) / 2, rect.y() + (rect.height() - s) / 2, s, s), self._state())

    def mousePressEvent(self, e):
        if self.logicalIndexAt(e.position().toPoint()) == 0:
            self.toggled.emit()
            self.viewport().update()
            return
        super().mousePressEvent(e)


class TypePillDelegate(QStyledItemDelegate):
    """Paints a column's text as a small muted pill (the file type in link lists); its 27 px height
    sets the row height of the list."""

    ROW = 27

    def sizeHint(self, option, index):
        return QSize(super().sizeHint(option, index).width() + 20, self.ROW)

    def paint(self, painter, option, index):
        opt = QStyleOptionViewItem(option)
        self.initStyleOption(opt, index)
        text = opt.text
        opt.text = ""
        opt.widget.style().drawControl(QStyle.CE_ItemViewItem, opt, painter, opt.widget)
        if not text:
            return
        f = QFont(option.font)
        f.setPointSizeF(f.pointSizeF() * 11.5 / 12)
        f.setWeight(QFont.DemiBold)
        fm = QFontMetrics(f)
        h = min(22, option.rect.height() - 4)
        r = QRectF(option.rect.x() + 6, option.rect.y() + (option.rect.height() - h) / 2, fm.horizontalAdvance(text) + 20, h)
        painter.save()
        painter.setRenderHint(QPainter.Antialiasing)
        path = QPainterPath()
        path.addRoundedRect(r, h / 2, h / 2)
        painter.fillPath(path, _c("track"))
        painter.setFont(f)
        painter.setPen(_c("mu"))
        painter.drawText(r, Qt.AlignCenter, text)
        painter.restore()


class ListDelegate(QStyledItemDelegate):
    """Status pills, a tint on checked rows and an accent bar on the current row."""

    def __init__(self, model, parent=None):
        super().__init__(parent)
        self.model = model
        self.row_height = 32

    def sizeHint(self, option, index):
        s = super().sizeHint(option, index)
        return QSize(s.width(), self.row_height)

    def paint(self, painter, option, index):
        row_checked = self.model.is_checked_row(index.row())
        selected = bool(option.state & QStyle.State_Selected)
        if row_checked and not selected:
            painter.fillRect(option.rect, _alpha(_c("acsel"), 150))
        key = self.model.columns[index.column()]
        if key == "status":
            opt = QStyleOptionViewItem(option)
            self.initStyleOption(opt, index)
            opt.text = ""
            opt.widget.style().drawControl(QStyle.CE_ItemViewItem, opt, painter, opt.widget)
            self._status(painter, option.rect, index.data(STATUS_ROLE), option.font)
        else:
            super().paint(painter, option, index)
        if selected and index.column() == 0:
            painter.fillRect(QRect(option.rect.x(), option.rect.y(), 3, option.rect.height()), _c("ac"))

    def _status(self, p: QPainter, rect: QRect, info, font: QFont) -> None:
        if not info:
            return
        kind, progress, text = info
        f = QFont(font)
        f.setBold(True)
        f.setPointSizeF(font.pointSizeF() * 0.88)
        if kind in ("run", "pause"):
            col = _c("ac") if kind == "run" else _c("fa")
            pill(p, rect, text, _c("tx"), _c("track"), mono_font(f), progress, col, width=150)
        elif kind == "done":
            pill(p, rect, "✓ " + text, _c("ok"), _alpha(_c("ok"), 34), f)
        elif kind == "error":
            pill(p, rect, "! " + text, _c("bad"), _alpha(_c("bad"), 30), f)
        elif kind == "queue":
            pill(p, rect, text, _c("mu"), _c("track"), f)


class TreeDelegate(QStyledItemDelegate):
    """Category tree rows with a count badge on the right."""

    def paint(self, painter, option, index):
        super().paint(painter, option, index)
        n = index.data(COUNT_ROLE)
        if not n:
            return
        selected = bool(option.state & QStyle.State_Selected)
        f = mono_font(option.font)
        f.setPointSizeF(option.font.pointSizeF() * 0.82)
        f.setBold(True)
        text = str(n)
        fm = QFontMetrics(f)
        w, h = fm.horizontalAdvance(text) + 14, fm.height() + 2
        r = QRectF(option.rect.right() - w - 6, option.rect.center().y() - h / 2 + 1, w, h)
        painter.save()
        painter.setRenderHint(QPainter.Antialiasing)
        path = QPainterPath()
        path.addRoundedRect(r, h / 2, h / 2)
        painter.fillPath(path, _c("ac") if selected else _c("track"))
        painter.setFont(f)
        painter.setPen(QColor("#FFFFFF") if selected else _c("mu"))
        painter.drawText(r, Qt.AlignCenter, text)
        painter.restore()
