"""Browser integration: register the native messaging host and export the extension.

Chromium-family browsers and Firefox look for ``<HOST_NAME>.json`` in their
NativeMessagingHosts folders; that file points at an executable which runs
``lider.native_host``. The extension itself ships inside the package
(``browser_extension/``) and is exported per browser family with the right manifest.
"""

from __future__ import annotations

import json
import os
import shlex
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

from .core.settings import data_dir

HOST_NAME = "io.github.davidaprilio.lid"
CHROMIUM_EXTENSION_ID = "hljibhdjbmpphbjhbfgiejnijpeobbne"
FIREFOX_EXTENSION_ID = "lid@davidaprilio.github.io"
EXTENSION_SOURCE = Path(__file__).parent / "browser_extension"
DESCRIPTION = "LID (Linux Internet Downloader) browser integration"


@dataclass(frozen=True)
class Browser:
    name: str
    config: str  # profile folder
    family: str  # chromium | firefox
    hosts_dir: str  # NativeMessagingHosts folder
    root: str = "config"  # paths are relative to $XDG_CONFIG_HOME ("config") or to the home folder ("home")


BROWSERS = [
    Browser("Google Chrome", "google-chrome", "chromium", "google-chrome/NativeMessagingHosts"),
    Browser("Chromium", "chromium", "chromium", "chromium/NativeMessagingHosts"),
    Browser("Brave", "BraveSoftware/Brave-Browser", "chromium", "BraveSoftware/Brave-Browser/NativeMessagingHosts"),
    Browser("Microsoft Edge", "microsoft-edge", "chromium", "microsoft-edge/NativeMessagingHosts"),
    Browser("Vivaldi", "vivaldi", "chromium", "vivaldi/NativeMessagingHosts"),
    Browser("Opera", "opera", "chromium", "opera/NativeMessagingHosts"),
    Browser("Firefox", ".mozilla", "firefox", ".mozilla/native-messaging-hosts", "home"),
    Browser("LibreWolf", ".librewolf", "firefox", ".librewolf/native-messaging-hosts", "home"),
]


def _base(b: Browser, home: Path | None) -> Path:
    """Chromium browsers follow $XDG_CONFIG_HOME for their default folder; Firefox uses the home folder."""
    if b.root == "home":
        return home or Path.home()
    if home is not None:
        return home / ".config"
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")


# System-wide folders used by the .deb and Arch packages.
SYSTEM_HOST_DIRS = {
    "chromium": ["/etc/opt/chrome/native-messaging-hosts", "/etc/chromium/native-messaging-hosts",
                 "/etc/opt/edge/native-messaging-hosts", "/etc/opt/brave/native-messaging-hosts"],
    "firefox": ["/usr/lib/mozilla/native-messaging-hosts"],
}


def host_manifest(family: str, host_path: str) -> dict:
    manifest = {"name": HOST_NAME, "description": DESCRIPTION, "path": host_path, "type": "stdio"}
    if family == "firefox":
        manifest["allowed_extensions"] = [FIREFOX_EXTENSION_ID]
    else:
        manifest["allowed_origins"] = [f"chrome-extension://{CHROMIUM_EXTENSION_ID}/"]
    return manifest


def wrapper_script() -> str:
    """Shell script the browser runs; it knows how this copy of LID is started."""
    appimage = os.environ.get("APPIMAGE")
    if appimage:
        run = f"exec {shlex.quote(appimage)} --native-host"
        launch = shlex.join([appimage, "--minimized"])
    else:
        pkg_parent = str(Path(__file__).resolve().parent.parent)
        run = f"PYTHONPATH={shlex.quote(pkg_parent)} exec {shlex.quote(sys.executable)} -m lider.native_host"
        installed = shutil.which("lider")
        launch = shlex.join([installed, "--minimized"] if installed else
                            ["env", f"PYTHONPATH={pkg_parent}", sys.executable, "-m", "lider", "--minimized"])
    return ("#!/bin/sh\n"
            "# Native messaging host for the LID browser extension. Written by LID; safe to delete.\n"
            f"export LIDER_LAUNCH={shlex.quote(launch)}\n"
            f'{run} "$@"\n')


def host_executable() -> Path:
    return data_dir() / "native-host" / "lider-native-host"


def detected(home: Path | None = None) -> list[Browser]:
    return [b for b in BROWSERS if (_base(b, home) / b.config).is_dir()]


def is_installed(browser: Browser, home: Path | None = None) -> bool:
    return (_base(browser, home) / browser.hosts_dir / f"{HOST_NAME}.json").is_file()


def install(browsers: list[Browser], home: Path | None = None) -> Path:
    """Write the wrapper once and a host manifest for every browser. Returns the wrapper path."""
    exe = host_executable()
    exe.parent.mkdir(parents=True, exist_ok=True)
    exe.write_text(wrapper_script())
    exe.chmod(0o755)
    for b in browsers:
        folder = _base(b, home) / b.hosts_dir
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f"{HOST_NAME}.json").write_text(json.dumps(host_manifest(b.family, str(exe)), indent=2) + "\n")
    return exe


def uninstall(browsers: list[Browser], home: Path | None = None) -> None:
    for b in browsers:
        (_base(b, home) / b.hosts_dir / f"{HOST_NAME}.json").unlink(missing_ok=True)


def export_extension(family: str, dest: Path) -> Path:
    """Copy the extension for one browser family into ``dest`` with its manifest.json."""
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(EXTENSION_SOURCE, dest, ignore=shutil.ignore_patterns("manifest.*.json", "__pycache__", "*.py"))
    shutil.copy(EXTENSION_SOURCE / f"manifest.{family}.json", dest / "manifest.json")
    return dest


def extension_folder(family: str) -> Path:
    """Exported copy in the user's data folder; browsers load it as an unpacked extension."""
    return export_extension(family, data_dir() / "browser-extension" / family)


def write_system_files(root: str, host_path: str, python: str, pythonpath: str = "") -> None:
    """Used by the package builds: host script, system-wide host manifests, exported extensions."""
    rootp = Path(root)
    exe = rootp / host_path.lstrip("/")
    exe.parent.mkdir(parents=True, exist_ok=True)
    env = f"PYTHONPATH={pythonpath} " if pythonpath else ""
    exe.write_text("#!/bin/sh\n# Native messaging host for the LID browser extension.\n"
                   f'{env}exec {python} -m lider.native_host "$@"\n')
    exe.chmod(0o755)
    for family, folders in SYSTEM_HOST_DIRS.items():
        for folder in folders:
            target = rootp / folder.lstrip("/") / f"{HOST_NAME}.json"
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(json.dumps(host_manifest(family, host_path), indent=2) + "\n")
            target.chmod(0o644)
    for family in ("chromium", "firefox"):
        export_extension(family, rootp / "usr/share/lider/browser-extension" / family)


def main(argv: list[str]) -> int:
    """``lider --install-browser-integration`` / ``--uninstall-browser-integration``."""
    found = detected()
    if "--uninstall-browser-integration" in argv:
        uninstall(BROWSERS)
        print("Removed the LID native messaging host from every browser.")
        return 0
    if not found:
        print("No supported browser profile found (Chrome, Chromium, Brave, Edge, Vivaldi, Opera, Firefox).")
        return 1
    exe = install(found)
    print(f"Native messaging host: {exe}")
    for b in found:
        print(f"  registered for {b.name}")
    for family in sorted({b.family for b in found}):
        print(f"Extension for {family}: {extension_folder(family)}")
    print("Load that folder as an unpacked extension (see Options -> General -> Browser integration).")
    return 0
