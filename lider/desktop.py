"""Desktop integration for a copy of LID that no package installed (an AppImage or a checkout):
the application-menu entry, the autostart entry, and keeping them, and the browser helper,
pointed at the copy that is running now, so they keep working after an AppImage is replaced."""

from __future__ import annotations

import os
import re
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

ICON_SOURCE = Path(__file__).parent / "ui" / "assets" / "icons" / "app.svg"
SYSTEM_ENTRY = Path("/usr/share/applications/lider.desktop")  # the .deb and Arch packages
_MARK = "X-LID-Written-By=LID"  # entries LID wrote, which it may rewrite or remove
_RESERVED = set(" \t\n\"'\\><~|&;$*?#()`")


def _data_home() -> Path:
    return Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local" / "share")


def _config_home() -> Path:
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")


def menu_entry_path() -> Path:
    return _data_home() / "applications" / "lider.desktop"


def autostart_path() -> Path:
    return _config_home() / "autostart" / "lider.desktop"


def icon_path() -> Path:
    return _data_home() / "icons" / "hicolor" / "scalable" / "apps" / "lider.svg"


def appimage() -> str:
    return os.environ.get("APPIMAGE", "")


def launch_argv() -> list[str]:
    """How this copy of LID is started: the AppImage, the installed ``lider``, or this checkout."""
    if appimage():
        return [appimage()]
    installed = shutil.which("lider")
    if installed:
        return [installed]
    pkg_parent = str(Path(__file__).resolve().parent.parent)
    return ["env", f"PYTHONPATH={pkg_parent}", sys.executable, "-m", "lider"]


def launch_command(*extra: str) -> str:
    """``launch_argv`` plus ``extra`` as a shell command line."""
    return shlex.join([*launch_argv(), *extra])


def _exec_arg(arg: str) -> str:
    """One argument of a desktop entry's Exec key (quoting and escaping of the Desktop Entry spec)."""
    arg = arg.replace("%", "%%")
    if arg and not set(arg) & _RESERVED:
        return arg
    quoted = '"' + re.sub(r'(["`$\\])', r"\\\1", arg) + '"'
    return _string(quoted)  # the key's value is a string: its backslashes are escaped again


def _string(value: str) -> str:
    """A desktop entry value of type string."""
    return value.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")


def _exec(*extra: str) -> str:
    return " ".join(_exec_arg(a) for a in [*launch_argv(), *extra])


def menu_entry_text() -> str:
    argv = launch_argv()
    try_exec = argv[0] if argv[0] != "env" else sys.executable
    return ("[Desktop Entry]\n"
            "Type=Application\n"
            "Name=LID\n"
            "GenericName=Download Manager\n"
            "Comment=Linux Internet Downloader: accelerated multi-connection downloads\n"
            f"Exec={_exec()} %U\n"
            f"TryExec={_string(try_exec)}\n"  # a plain path; hides the entry once the AppImage is gone
            "Icon=lider\n"
            "Terminal=false\n"
            "Categories=Network;FileTransfer;Qt;\n"
            "MimeType=x-scheme-handler/http;x-scheme-handler/https;x-scheme-handler/ftp;\n"
            f"{_MARK}\n")


def autostart_text() -> str:
    return ("[Desktop Entry]\n"
            "Type=Application\n"
            "Name=LID\n"
            f"Exec={_exec('--minimized')}\n"
            "Icon=lider\n"
            "Terminal=false\n"
            "X-GNOME-Autostart-enabled=true\n")


def system_installed() -> bool:
    """A package put LID in the application menu already."""
    return SYSTEM_ENTRY.exists()


def in_menu() -> bool:
    return menu_entry_path().is_file()


def listed_elsewhere() -> bool:
    """Another tool (AppImageLauncher, Gear Lever, appimaged) already gave this AppImage a menu entry."""
    folder = menu_entry_path().parent
    if not appimage() or not folder.is_dir():
        return False
    for entry in folder.glob("*.desktop"):
        if entry.name != menu_entry_path().name:
            try:
                if appimage() in entry.read_text(errors="replace"):
                    return True
            except OSError:
                pass
    return False


def add_to_menu() -> None:
    icon = icon_path()
    icon.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ICON_SOURCE, icon)
    entry = menu_entry_path()
    entry.parent.mkdir(parents=True, exist_ok=True)
    entry.write_text(menu_entry_text())
    _update_menus(entry.parent)


def remove_from_menu() -> None:
    menu_entry_path().unlink(missing_ok=True)
    icon_path().unlink(missing_ok=True)
    _update_menus(menu_entry_path().parent)


def _update_menus(folder: Path) -> None:
    """Desktops watch the folder; the cache of x-scheme-handler entries needs this nudge."""
    tool = shutil.which("update-desktop-database")
    if tool and folder.is_dir():
        subprocess.run([tool, str(folder)], capture_output=True, timeout=20, check=False)


def autostart_enabled() -> bool:
    return autostart_path().is_file()


def set_autostart(on: bool) -> None:
    path = autostart_path()
    if on:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(autostart_text())
    else:
        path.unlink(missing_ok=True)


def refresh() -> list[str]:
    """Point the files LID wrote at the copy that runs now (an AppImage that moved or was
    updated, say). Returns what was rewritten."""
    from .browser import host_executable, wrapper_script

    if launch_argv()[0] == "env":  # a checkout: the entries belong to the installed copy
        return []
    done = []
    files = [("autostart", autostart_path(), autostart_text), ("browser helper", host_executable(), wrapper_script)]
    # An entry without the mark is the user's (a menu editor saves its changes to the same file).
    if in_menu() and _MARK in menu_entry_path().read_text():
        if system_installed():
            remove_from_menu()  # the package's own entry takes over
            done.append("menu")
        else:
            files.insert(0, ("menu", menu_entry_path(), menu_entry_text))
    for name, path, text in files:
        if path.is_file():
            wanted = text()
            if path.read_text() != wanted:
                path.write_text(wanted)
                done.append(name)
    return done
