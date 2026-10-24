"""The three LID themes (Ember, Ember Dark, Dark): palette, Qt style sheet and indicator images.

Every theme runs on the Fusion style so LID looks the same on every distribution. Widgets that
paint themselves (delegates, the details pane) read their colours from ``colors()``.
"""

from __future__ import annotations

import atexit
import shutil
import tempfile
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QGuiApplication, QPalette
from PySide6.QtWidgets import QApplication, QStyleFactory

from . import icons

THEMES: dict[str, dict[str, str]] = {
    "ember": dict(
        win="#F7F5F2", pane="#FFFFFF", alt="#FBFAF8", hdr="#F9F7F4", line="#E7E2DA", line2="#D8D1C6",
        tx="#2A2420", mu="#6B625A", fa="#9A9087", ac="#EA580C", acsel="#FDEEE4", actx="#C2410C",
        ok="#0D9488", bad="#DC2626", track="#EFEAE3", input="#FFFFFF", btn="#FFFFFF",
        ink="#1E293B", icon_ac="#EA580C", title="#EEEAE4"),
    "ember-dark": dict(
        win="#1A1714", pane="#141210", alt="#181512", hdr="#1D1915", line="#2E2925", line2="#40382F",
        tx="#EDE6DF", mu="#AEA399", fa="#7C7168", ac="#F97316", acsel="#3A2417", actx="#FDBA74",
        ok="#2DD4BF", bad="#F87171", track="#2B2520", input="#100E0C", btn="#221E1A",
        ink="#E2E8F0", icon_ac="#F97316", title="#221E1A"),
    "dark": dict(
        win="#1B1E24", pane="#15181D", alt="#181B21", hdr="#1D2027", line="#2B3039", line2="#3A404B",
        tx="#E2E8F0", mu="#A3ADBD", fa="#6E7787", ac="#F97316", acsel="#33261F", actx="#FDBA74",
        ok="#2DD4BF", bad="#F87171", track="#2A2F38", input="#111318", btn="#252932",
        ink="#E2E8F0", icon_ac="#F97316", title="#23262D"),
}
TITLES = {"ember": "Ember", "ember-dark": "Ember Dark", "dark": "Dark"}
DARK_THEMES = ("ember-dark", "dark")

_current = "ember"
_assets: Path | None = None


def colors(name: str | None = None) -> dict[str, str]:
    return THEMES[name or _current]


def current() -> str:
    return _current


def is_dark(name: str | None = None) -> bool:
    return (name or _current) in DARK_THEMES


def system_is_dark() -> bool:
    hints = QGuiApplication.styleHints()
    return hasattr(hints, "colorScheme") and hints.colorScheme() == Qt.ColorScheme.Dark


def global_menu_available() -> bool:
    """True when the desktop shows application menus in a global menu (KDE, Unity and others)."""
    try:
        from PySide6.QtDBus import QDBusConnection
    except ImportError:
        return False
    bus = QDBusConnection.sessionBus()
    if not bus.isConnected():
        return False
    reply = bus.interface().isServiceRegistered("com.canonical.AppMenu.Registrar")
    return bool(reply.value() if hasattr(reply, "value") else reply)


def resolve(settings) -> str:
    """The theme to use now: the chosen one, or the light/dark pair when following the system."""
    if settings.follow_system:
        return settings.dark_theme if system_is_dark() else "ember"
    return settings.theme if settings.theme in THEMES else "ember"


def _rgba(hex_color: str, alpha: float) -> str:
    r, g, b = (int(hex_color[i:i + 2], 16) for i in (1, 3, 5))
    return f"rgba({r}, {g}, {b}, {alpha})"


def _write_assets(c: dict[str, str]) -> Path:
    """Glyphs the style sheet points at (QSS cannot take inline SVG)."""
    global _assets
    if _assets is None:
        _assets = Path(tempfile.mkdtemp(prefix="lider-theme-"))
        atexit.register(shutil.rmtree, _assets, True)
    svg = ('<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" '
           'stroke="{c}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round">{p}</svg>')
    glyphs = {
        "check": ("#FFFFFF", 3, '<path d="M5 12.5l4.5 4.5L19 7.5"/>'),
        "dash": ("#FFFFFF", 3, '<path d="M7 12h10"/>'),
        "down": (c["fa"], 2.4, '<path d="M7 10l5 5 5-5"/>'),
        "up": (c["fa"], 2.4, '<path d="M7 14l5-5 5 5"/>'),
        "right": (c["fa"], 2.4, '<path d="M10 7l5 5-5 5"/>'),
        "close": (c["fa"], 2.2, '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>'),
    }
    for name, (col, w, p) in glyphs.items():
        (_assets / f"{name}.svg").write_text(svg.format(c=col, w=w, p=p))
    return _assets


def stylesheet(name: str) -> str:
    c = THEMES[name]
    a = _write_assets(c).as_posix()
    return f"""
* {{ outline: none; }}
QMainWindow, QDialog {{ background: {c['win']}; color: {c['tx']}; }}
QWidget {{ color: {c['tx']}; }}
QToolTip {{ background: {c['pane']}; color: {c['tx']}; border: 1px solid {c['line2']}; border-radius: 6px; padding: 4px 8px; }}

QMenuBar {{ background: {c['win']}; padding: 2px 4px; }}
QMenuBar::item {{ padding: 5px 10px; border-radius: 6px; background: transparent; }}
QMenuBar::item:selected {{ background: {c['acsel']}; color: {c['actx']}; }}
QToolBar#menubar {{ background: {c['win']}; border: none; padding: 2px 4px 0; spacing: 0; }}
QToolBar#menubar QMenuBar {{ background: transparent; }}
QMenu {{ background: {c['pane']}; border: 1px solid {c['line2']}; border-radius: 10px; padding: 6px; }}
QMenu::item {{ padding: 6px 28px 6px 12px; border-radius: 7px; }}
QMenu::item:selected {{ background: {c['acsel']}; color: {c['actx']}; }}
QMenu::item:disabled {{ color: {c['fa']}; }}
QMenu::separator {{ height: 1px; background: {c['line']}; margin: 4px 6px; }}
QMenu::icon {{ padding-left: 8px; }}
QMenu::indicator {{ width: 14px; height: 14px; left: 6px; }}
QMenu::indicator:checked {{ image: url({a}/check.svg); background: {c['ac']}; border-radius: 4px; }}

QToolBar#toolbar {{ background: {c['win']}; border: none; border-bottom: 1px solid {c['line']}; padding: 8px 12px; spacing: 8px; }}
QToolBar::separator {{ background: {c['line2']}; width: 1px; margin: 6px 4px; }}
QToolButton {{ border-radius: 9px; padding: 6px 10px; background: transparent; }}
QToolButton:hover {{ background: {c['acsel']}; }}
QToolButton#pill, QToolButton#danger {{ background: {c['pane']}; border: 1px solid {c['line2']}; border-radius: 17px;
    padding: 0; font-weight: 600; }}
QToolButton#pill:hover {{ border-color: {c['ac']}; background: {c['pane']}; }}
QToolButton#pill:disabled, QToolButton#danger:disabled {{ background: {c['win']}; color: {c['fa']}; border-color: {c['line']}; }}
QToolButton#danger {{ color: {c['bad']}; }}
QToolButton#danger:hover {{ border-color: {c['bad']}; background: {c['pane']}; }}
QToolButton#danger::menu-button {{ subcontrol-origin: padding; subcontrol-position: right center; border: none; width: 31px;
    border-top-right-radius: 17px; border-bottom-right-radius: 17px; }}
QToolButton#danger::menu-arrow {{ image: none; }}
QToolButton#primary {{ background: {c['ac']}; color: #FFFFFF; border-radius: 19px; padding: 0; font-weight: 700; }}
QToolButton#primary:hover {{ background: {c['actx']}; }}
QToolButton#ghost {{ font-weight: 600; padding: 0; border-radius: 9px; }}
QFrame#chip {{ background: {c['acsel']}; border-radius: 15px; }}
QFrame#chip QLabel {{ color: {c['actx']}; font-weight: 700; background: transparent; }}
QToolButton#chipClose {{ border-radius: 10px; padding: 0; background: transparent; }}
QToolButton#chipClose:hover {{ background: {c['ac']}; }}
QToolButton#close {{ padding: 3px; border-radius: 6px; }}
QToolButton#compact {{ background: {c['btn']}; border: 1px solid {c['line2']}; border-radius: 7px; padding: 0; font-weight: 600; }}
QToolButton#compact:hover {{ border-color: {c['ac']}; background: {c['btn']}; }}
QToolButton#compact:disabled {{ color: {c['fa']}; background: {c['win']}; border-color: {c['line']}; }}
QFrame#segmented {{ background: {c['track']}; border-radius: 15px; }}
QToolButton#seg {{ border-radius: 12px; padding: 4px 13px; color: {c['mu']}; font-weight: 600; }}
QToolButton#seg:checked {{ background: {c['pane']}; color: {c['tx']}; }}

QPushButton {{ background: {c['btn']}; border: 1px solid {c['line2']}; border-radius: 16px; padding: 6px 14px; min-height: 18px; font-weight: 600; }}
QPushButton:hover {{ border-color: {c['ac']}; }}
QPushButton[primary="true"] {{ background: {c['ac']}; border-color: {c['ac']}; color: #FFFFFF; }}
QPushButton[primary="true"]:hover {{ background: {c['actx']}; }}
QPushButton:disabled {{ color: {c['fa']}; background: {c['win']}; border-color: {c['line']}; }}
QPushButton#small {{ padding: 5px 0; border-radius: 10px; }}
QPushButton::menu-indicator {{ image: url({a}/down.svg); width: 11px; subcontrol-position: right center; right: 8px; }}

QLineEdit, QPlainTextEdit, QTextEdit, QSpinBox, QDoubleSpinBox, QComboBox, QDateEdit, QTimeEdit, QDateTimeEdit {{
    background: {c['input']}; border: 1px solid {c['line2']}; border-radius: 8px; padding: 5px 8px;
    selection-background-color: {c['ac']}; selection-color: #FFFFFF; }}
QLineEdit:focus, QPlainTextEdit:focus, QTextEdit:focus, QSpinBox:focus, QComboBox:focus, QDateEdit:focus, QTimeEdit:focus, QDateTimeEdit:focus {{
    border: 1px solid {c['ac']}; }}
QLineEdit:disabled, QSpinBox:disabled, QComboBox:disabled, QTimeEdit:disabled, QDateEdit:disabled {{ color: {c['fa']}; background: {c['win']}; }}
QComboBox::drop-down, QDateEdit::drop-down, QDateTimeEdit::drop-down {{ border: none; width: 22px; }}
QComboBox::down-arrow, QDateEdit::down-arrow, QDateTimeEdit::down-arrow {{ image: url({a}/down.svg); width: 12px; height: 12px; }}
QComboBox QAbstractItemView {{ background: {c['pane']}; border: 1px solid {c['line2']}; border-radius: 8px; padding: 4px;
    selection-background-color: {c['acsel']}; selection-color: {c['actx']}; }}
QSpinBox::up-button, QSpinBox::down-button, QTimeEdit::up-button, QTimeEdit::down-button, QDateEdit::up-button, QDateEdit::down-button {{
    border: none; width: 18px; }}
QSpinBox::up-arrow, QTimeEdit::up-arrow, QDateEdit::up-arrow {{ image: url({a}/up.svg); width: 10px; height: 10px; }}
QSpinBox::down-arrow, QTimeEdit::down-arrow, QDateEdit::down-arrow {{ image: url({a}/down.svg); width: 10px; height: 10px; }}

QCheckBox, QRadioButton {{ spacing: 8px; }}
QCheckBox::indicator, QTreeView::indicator, QTreeWidget::indicator, QListWidget::indicator, QGroupBox::indicator {{
    width: 16px; height: 16px; border-radius: 4px; border: 1.5px solid {c['line2']}; background: {c['input']}; }}
QCheckBox::indicator:hover, QTreeView::indicator:hover {{ border-color: {c['ac']}; }}
QCheckBox::indicator:checked, QTreeView::indicator:checked, QTreeWidget::indicator:checked, QListWidget::indicator:checked, QGroupBox::indicator:checked {{
    background: {c['ac']}; border-color: {c['ac']}; image: url({a}/check.svg); }}
QCheckBox::indicator:indeterminate, QTreeView::indicator:indeterminate, QTreeWidget::indicator:indeterminate {{
    background: {c['ac']}; border-color: {c['ac']}; image: url({a}/dash.svg); }}
QCheckBox::indicator:disabled {{ background: {c['win']}; border-color: {c['line']}; }}
QCheckBox::indicator:checked:disabled {{ background: {c['acsel']}; border-color: {c['acsel']}; }}
QCheckBox:disabled, QRadioButton:disabled {{ color: {c['fa']}; }}
QRadioButton::indicator {{ width: 16px; height: 16px; border-radius: 8px; border: 1.5px solid {c['line2']}; background: {c['input']}; }}
QRadioButton::indicator:checked {{ border-color: {c['ac']};
    background: qradialgradient(cx:0.5, cy:0.5, radius:0.5, fx:0.5, fy:0.5, stop:0 #FFFFFF, stop:0.36 #FFFFFF, stop:0.42 {c['ac']}, stop:1 {c['ac']}); }}

QGroupBox {{ background: {c['pane']}; border: 1px solid {c['line']}; border-radius: 10px; margin-top: 22px; padding: 12px 10px 10px; }}
QGroupBox::title {{ subcontrol-origin: margin; left: 4px; top: 2px; color: {c['fa']}; font-weight: 700; }}

QTabWidget::pane {{ border: none; border-top: 1px solid {c['line']}; top: -1px; }}
QTabBar::tab {{ background: transparent; color: {c['mu']}; padding: 8px 12px; margin-right: 2px; border: none; font-weight: 600; }}
QTabBar::tab:selected {{ color: {c['tx']}; border-bottom: 2px solid {c['ac']}; }}
QTabBar::tab:hover:!selected {{ color: {c['tx']}; }}

QTreeView, QTreeWidget, QListWidget, QTableView {{ background: {c['pane']}; alternate-background-color: {c['alt']};
    border: 1px solid {c['line']}; border-radius: 9px; selection-background-color: {c['acsel']}; selection-color: {c['tx']}; }}
QTreeView::item, QListWidget::item {{ padding: 2px 4px; border: none; }}
QTreeView::item:selected, QListWidget::item:selected {{ background: {c['acsel']}; color: {c['tx']}; }}
QTreeView::item:hover:!selected {{ background: {c['alt']}; }}
QTreeView#downloads {{ border: none; border-radius: 0; }}
QTreeView#downloads::item {{ padding: 0 6px; }}
QTreeWidget#categories {{ background: {c['win']}; border: none; border-radius: 0; padding: 0 6px; }}
QTreeWidget#categories::item {{ border-radius: 6px; padding: 3px 4px; }}
QTreeWidget#categories::item:selected {{ background: {c['acsel']}; color: {c['actx']}; font-weight: 600; }}
QTreeView::branch {{ background: transparent; }}
QTreeView::branch:has-children:!has-siblings:closed, QTreeView::branch:closed:has-children:has-siblings {{ image: url({a}/right.svg); }}
QTreeView::branch:open:has-children:!has-siblings, QTreeView::branch:open:has-children:has-siblings {{ image: url({a}/down.svg); }}

QHeaderView {{ background: {c['hdr']}; border: none; }}
QHeaderView::section {{ background: {c['hdr']}; color: {c['mu']}; border: none; border-right: 1px solid {c['line']};
    border-bottom: 1px solid {c['line']}; padding: 6px 8px; font-weight: 700; }}
QHeaderView::down-arrow {{ image: url({a}/down.svg); width: 10px; }}
QHeaderView::up-arrow {{ image: url({a}/up.svg); width: 10px; }}

QScrollBar:vertical {{ background: transparent; width: 11px; margin: 2px; }}
QScrollBar::handle:vertical {{ background: {c['line2']}; border-radius: 4px; min-height: 28px; }}
QScrollBar:horizontal {{ background: transparent; height: 11px; margin: 2px; }}
QScrollBar::handle:horizontal {{ background: {c['line2']}; border-radius: 4px; min-width: 28px; }}
QScrollBar::add-line, QScrollBar::sub-line {{ width: 0; height: 0; }}
QScrollBar::add-page, QScrollBar::sub-page {{ background: transparent; }}

QProgressBar {{ background: {c['track']}; border: none; border-radius: 6px; height: 12px; text-align: center; color: transparent; }}
QProgressBar::chunk {{ background: {c['ac']}; border-radius: 6px; }}
QSplitter::handle {{ background: {c['line']}; }}
QSplitter::handle:horizontal {{ width: 1px; }}
QSplitter::handle:vertical {{ height: 1px; }}
QSplitter#sidebar::handle {{ width: 6px; background: {c['win']}; border-right: 1px solid {c['line']}; }}
QSplitter#sidebar::handle:hover {{ border-right: 2px solid {c['ac']}; }}
QSplitter#detailsSplit::handle {{ height: 6px; background: {c['pane']}; border-bottom: 1px solid {c['line2']}; }}
QSplitter#detailsSplit::handle:hover {{ border-bottom: 2px solid {c['ac']}; }}
QStatusBar {{ background: {c['win']}; border-top: 1px solid {c['line']}; color: {c['mu']}; }}
QStatusBar::item {{ border: none; }}
QStatusBar QLabel {{ color: {c['mu']}; padding: 0 8px; }}

QFrame#footer {{ background: {c['win']}; border-top: 1px solid {c['line']}; }}
QLabel#formLabel {{ color: {c['mu']}; }}
QLabel#noteBox {{ background: {c['track']}; color: {c['mu']}; border-radius: 9px; padding: 9px 12px; }}
QLabel#pillOk, QLabel#pillBad, QLabel#pillAccent, QLabel#pillMuted {{ border-radius: 10px; padding: 2px 10px; font-weight: 600; }}
QLabel#pillOk {{ background: {_rgba(c['ok'], 0.14)}; color: {c['ok']}; }}
QLabel#pillBad {{ background: {_rgba(c['bad'], 0.12)}; color: {c['bad']}; }}
QLabel#pillAccent {{ background: {c['acsel']}; color: {c['actx']}; }}
QLabel#pillMuted {{ background: {c['track']}; color: {c['mu']}; }}
QPushButton#ghostBtn {{ background: transparent; border: none; color: {c['mu']}; }}
QPushButton#ghostBtn:hover {{ color: {c['tx']}; }}
QLabel#sectionTitle {{ color: {c['fa']}; font-weight: 700; letter-spacing: 1px; }}
QLabel#muted {{ color: {c['mu']}; }}
QLabel#faint {{ color: {c['fa']}; }}
QLabel#dialogTitle {{ font-weight: 700; }}
QLabel#bigNumber {{ font-weight: 700; }}
QLabel#link {{ color: {c['actx']}; }}
QFrame#panel {{ background: {c['pane']}; border: 1px solid {c['line']}; border-radius: 10px; }}
QFrame#tile {{ background: {c['win']}; border: 1px solid {c['line']}; border-radius: 12px; }}
QFrame#details {{ background: {c['pane']}; }}
QFrame#detailsTabs {{ background: {c['hdr']}; border: none; border-bottom: 1px solid {c['line']}; }}
QFrame#detailsTabs QTabBar::tab {{ padding: 6px 12px 5px 12px; margin: 0; border-bottom: 2px solid transparent; }}
QFrame#detailsTabs QTabBar::tab:selected {{ border-bottom-color: {c['ac']}; }}
QLabel#okText {{ color: {c['ok']}; }}
QFrame#tagInput {{ background: {c['input']}; border: 1px solid {c['line2']}; border-radius: 9px; }}
QFrame#tagInput[focused="true"] {{ border: 1px solid {c['ac']}; }}
QFrame#tagInput QLineEdit {{ border: none; background: transparent; padding: 2px; }}
QToolButton#tag {{ background: {c['acsel']}; color: {c['actx']}; border-radius: 13px; padding: 3px 4px 3px 10px; font-weight: 600; }}
QToolButton#tag:hover {{ border: 1px solid {c['ac']}; }}
"""


def palette(name: str) -> QPalette:
    c = THEMES[name]
    p = QPalette()
    q = QColor
    for role, col in ((QPalette.Window, c["win"]), (QPalette.WindowText, c["tx"]), (QPalette.Base, c["pane"]),
                      (QPalette.AlternateBase, c["alt"]), (QPalette.Text, c["tx"]), (QPalette.Button, c["btn"]),
                      (QPalette.ButtonText, c["tx"]), (QPalette.ToolTipBase, c["pane"]), (QPalette.ToolTipText, c["tx"]),
                      (QPalette.Highlight, c["ac"]), (QPalette.HighlightedText, "#FFFFFF"),
                      (QPalette.PlaceholderText, c["fa"]), (QPalette.Link, c["actx"]), (QPalette.Mid, c["line2"]),
                      (QPalette.Dark, c["line2"]), (QPalette.Light, c["pane"]), (QPalette.Midlight, c["line"])):
        p.setColor(role, q(col))
    for role in (QPalette.Text, QPalette.ButtonText, QPalette.WindowText):
        p.setColor(QPalette.Disabled, role, q(c["fa"]))
    return p


def apply(name: str) -> None:
    """Switch the whole application to theme ``name``."""
    global _current
    _current = name if name in THEMES else "ember"
    app = QApplication.instance()
    if not app.property("lider_fusion"):  # once: with a style sheet app.style() is a wrapper, not Fusion itself
        app.setStyle(QStyleFactory.create("Fusion"))
        app.setProperty("lider_fusion", True)
    sheet = stylesheet(_current)
    if app.styleSheet() != sheet:  # re-applying the same sheet would re-polish every widget for nothing
        app.setPalette(palette(_current))
        app.setStyleSheet(sheet)
    c = THEMES[_current]
    icons.set_colors(c["ink"], c["icon_ac"])
