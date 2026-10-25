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
import zipfile
from dataclasses import dataclass
from pathlib import Path

from . import __version__
from .core.settings import data_dir

HOST_NAME = "io.github.davidaprilio.lid"
# The id fixed by the "key" in manifest.chromium.json (unpacked installs and the GitHub zip).
CHROMIUM_EXTENSION_ID = "hljibhdjbmpphbjhbfgiejnijpeobbne"
# The id Microsoft Edge Add-ons gave the store build (Partner Center, "CRX ID").
EDGE_ADDONS_EXTENSION_ID = "ophlhhmpoaonhglimkemkgjifigpghej"
# Every Chromium extension id the helper accepts. Add the Chrome Web Store id here once that
# listing exists, unless the store kept the id above.
CHROMIUM_EXTENSION_IDS = (CHROMIUM_EXTENSION_ID, EDGE_ADDONS_EXTENSION_ID)
FIREFOX_EXTENSION_ID = "lid@davidaprilio.github.io"
EXTENSION_SOURCE = Path(__file__).parent / "browser_extension"
DESCRIPTION = "LID (Linux Internet Downloader) browser integration"

RELEASES_URL = "https://github.com/Davidaprilio/lid/releases/latest"
# Store pages; empty until the listings are published.
CHROME_WEB_STORE_URL = ""
FIREFOX_ADDONS_URL = ""
# Releases carry the signed lid-extension-firefox.xpi only when the CI has the AMO keys. Set this to
# True once they do; until then the dialog links the unsigned zip (temporary add-on).
FIREFOX_SIGNED_XPI = False
# Stable "latest release" links to the files built by ./packaging/build.sh extension.
FIREFOX_XPI_URL = RELEASES_URL + "/download/lid-extension-firefox.xpi"
FIREFOX_ZIP_URL = RELEASES_URL + "/download/lid-extension-firefox.zip"
FIREFOX_UPDATES_URL = RELEASES_URL + "/download/firefox-updates.json"
CHROMIUM_ZIP_URL = RELEASES_URL + "/download/lid-extension-chromium.zip"

# Chrome Web Store does not accept extensions that download from YouTube, and "key" is not
# allowed in uploads: the "store" variant leaves both out. YouTube stays available in the app.
STORE_DESCRIPTION = "Send downloads and links from the browser to LID (Linux Internet Downloader)."
YOUTUBE_FILES = ("youtube.js", "youtube.css")


@dataclass(frozen=True)
class Browser:
    name: str
    config: str  # profile folder
    family: str  # chromium | firefox
    hosts_dir: str  # NativeMessagingHosts folder
    root: str = "config"  # paths are relative to $XDG_CONFIG_HOME ("config") or to the home folder ("home")
    # Gecko browsers: the folder under $XDG_CONFIG_HOME they use when their folder in the home
    # directory does not exist (Firefox 147 and later; ~/.mozilla stays in use where it exists).
    xdg: str = ""


BROWSERS = [
    Browser("Google Chrome", "google-chrome", "chromium", "google-chrome/NativeMessagingHosts"),
    Browser("Chromium", "chromium", "chromium", "chromium/NativeMessagingHosts"),
    Browser("Brave", "BraveSoftware/Brave-Browser", "chromium", "BraveSoftware/Brave-Browser/NativeMessagingHosts"),
    Browser("Microsoft Edge", "microsoft-edge", "chromium", "microsoft-edge/NativeMessagingHosts"),
    Browser("Vivaldi", "vivaldi", "chromium", "vivaldi/NativeMessagingHosts"),
    Browser("Opera", "opera", "chromium", "opera/NativeMessagingHosts"),
    Browser("Firefox", ".mozilla", "firefox", ".mozilla/native-messaging-hosts", "home", xdg="mozilla"),
    Browser("LibreWolf", ".librewolf", "firefox", ".librewolf/native-messaging-hosts", "home", xdg="librewolf"),
]


def _config_dir(home: Path | None) -> Path:
    if home is not None:
        return home / ".config"
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")


def _base(b: Browser, home: Path | None) -> Path:
    """Chromium browsers follow $XDG_CONFIG_HOME for their default folder; Firefox uses the home folder."""
    return (home or Path.home()) if b.root == "home" else _config_dir(home)


def _paths(b: Browser, home: Path | None, legacy: bool | None = None) -> tuple[Path, Path]:
    """(profile folder, NativeMessagingHosts folder) of a browser.

    Firefox keeps ~/.mozilla where it exists (or with MOZ_LEGACY_HOME set) and otherwise uses
    $XDG_CONFIG_HOME/mozilla, with the profiles and native-messaging-hosts inside; ``legacy`` forces a side.
    """
    if b.xdg:
        if legacy is None:
            legacy = (_base(b, home) / b.config).is_dir() or bool(os.environ.get("MOZ_LEGACY_HOME"))
        if not legacy:
            folder = _config_dir(home) / b.xdg
            return folder, folder / Path(b.hosts_dir).relative_to(b.config)
    base = _base(b, home)
    return base / b.config, base / b.hosts_dir


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
        manifest["allowed_origins"] = [f"chrome-extension://{i}/" for i in CHROMIUM_EXTENSION_IDS]
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
    return [b for b in BROWSERS if _paths(b, home)[0].is_dir()]


def is_installed(browser: Browser, home: Path | None = None) -> bool:
    return (_paths(browser, home)[1] / f"{HOST_NAME}.json").is_file()


def install(browsers: list[Browser], home: Path | None = None) -> Path:
    """Write the wrapper once and a host manifest for every browser. Returns the wrapper path."""
    exe = host_executable()
    exe.parent.mkdir(parents=True, exist_ok=True)
    exe.write_text(wrapper_script())
    exe.chmod(0o755)
    for b in browsers:
        folder = _paths(b, home)[1]
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f"{HOST_NAME}.json").write_text(json.dumps(host_manifest(b.family, str(exe)), indent=2) + "\n")
    return exe


def uninstall(browsers: list[Browser], home: Path | None = None) -> None:
    for b in browsers:
        for legacy in ((True, False) if b.xdg else (None,)):  # Gecko: both places, whichever was used
            (_paths(b, home, legacy)[1] / f"{HOST_NAME}.json").unlink(missing_ok=True)


def export_extension(family: str, dest: Path, variant: str = "full", update_url: str = "") -> Path:
    """Copy the extension for one browser family into ``dest`` with its manifest.json.

    variant "store" drops the YouTube button and the manifest key (Chrome Web Store rules).
    update_url (Firefox, self-distributed .xpi) lets Firefox find newer signed versions.
    """
    if dest.exists():
        shutil.rmtree(dest)
    skip = ["manifest.*.json", "__pycache__", "*.py"] + (list(YOUTUBE_FILES) if variant == "store" else [])
    shutil.copytree(EXTENSION_SOURCE, dest, ignore=shutil.ignore_patterns(*skip))
    manifest = json.loads((EXTENSION_SOURCE / f"manifest.{family}.json").read_text())
    manifest["version"] = __version__
    if variant == "store":
        manifest.pop("key", None)
        manifest.pop("content_scripts", None)
        manifest["description"] = STORE_DESCRIPTION
    if update_url and family == "firefox":
        manifest["browser_specific_settings"]["gecko"]["update_url"] = update_url
    (dest / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return dest


def zip_folder(folder: Path, out: Path) -> Path:
    """Reproducible zip: sorted entries, fixed timestamps and permissions."""
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for path in sorted(p for p in folder.rglob("*") if p.is_file()):
            info = zipfile.ZipInfo(path.relative_to(folder).as_posix(), date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, path.read_bytes())
    return out


def firefox_updates_json(version: str, xpi_url: str) -> dict:
    """The update manifest Firefox reads from gecko.update_url."""
    return {"addons": {FIREFOX_EXTENSION_ID: {"updates": [{"version": version, "update_link": xpi_url}]}}}


def build_extensions(dist: str, work: str) -> None:
    """Release files for ./packaging/build.sh extension.

    dist/lid-extension-chromium.zip        unpacked install, fixed id (has the key)
    dist/lid-extension-chromium-store.zip  Chrome Web Store / Edge Add-ons upload
    dist/lid-extension-firefox.zip         addons.mozilla.org listed upload / temporary loading
    work/firefox-unlisted/                 source for "web-ext sign --channel unlisted"
    dist/firefox-updates.json              update manifest for the self-distributed .xpi
    """
    d, w = Path(dist), Path(work)
    zip_folder(export_extension("chromium", w / "chromium"), d / "lid-extension-chromium.zip")
    zip_folder(export_extension("chromium", w / "chromium-store", "store"), d / "lid-extension-chromium-store.zip")
    zip_folder(export_extension("firefox", w / "firefox"), d / "lid-extension-firefox.zip")
    export_extension("firefox", w / "firefox-unlisted", update_url=FIREFOX_UPDATES_URL)
    xpi = f"https://github.com/Davidaprilio/lid/releases/download/v{__version__}/lid-extension-firefox.xpi"
    (d / "firefox-updates.json").write_text(json.dumps(firefox_updates_json(__version__, xpi), indent=2) + "\n")


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
