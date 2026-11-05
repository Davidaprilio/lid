# LID: behaviour checklist for the Rust + Tauri port

Everything below was read line by line. Paths are relative to the repository root unless they say otherwise. To find the first-start prompt, the data folders and how browser requests are used, I also read `lider/core/settings.py`, `lider/ui/dialogs/menu.py`, `lider/ui/dialogs/browsers.py`, and parts of `lider/ui/dialogs/options.py` and `lider/ui/main_window.py`. Items marked **QUIRK** are current behaviour that looks unintended or easy to miss. Decide for each one whether to keep it or fix it.

---

## 0. Identity and constants
- [ ] App version `__version__ = "1.2.0"`. `APP_NAME="LID"`, `APP_LONG_NAME="Linux Internet Downloader"`, `PACKAGE_NAME="lider"` (`lider/__init__.py:3-6`). This is the only place the version is defined: `pyproject.toml:21`, `build.sh:26` and `release.yml:30,89` all read it with `sed -n 's/^__version__ = "\(.*\)"/\1/p'`. The port needs one equivalent version source, and CI and build must parse it.
- [ ] The command and package are named `lider`. The desktop file name is `lider` (`app.py:83`) and the icon name is `lider`.
- [ ] Native messaging host name: `io.github.davidaprilio.lid` (`browser.py:24`, `background.js:6`).
- [ ] Extension version `EXTENSION_VERSION = "1.0.0"` is separate from the app version (`browser.py:27`).
- [ ] Project URLs: `https://github.com/Davidaprilio/lid`, with releases at `.../releases/latest` (`browser.py:39`, `popup.js:7-8`).

## 1. CLI and process modes
- [ ] **Dispatch happens before any GUI code loads** (`__main__.py:3-15`, repeated in `app.py:63-71`):
  - `--native-host` anywhere in argv starts native-host mode. No GUI or Qt (in Rust, no Tauri/webview) may be initialised. Test `test_native_host_does_not_load_qt` enforces this.
  - Otherwise, `--install-browser-integration` or `--uninstall-browser-integration` anywhere in argv starts browser-integration CLI mode (`browser.py:251-268`).
  - Otherwise the GUI app starts.
  - The checks run in that order, so `--native-host` wins over the others.
- [ ] **Native-host mode must ignore the extra positional args the browser adds.** Chrome passes the origin `chrome-extension://<id>/` (and `--parent-window=` on some platforms). Firefox passes the manifest path and the extension id. The wrappers forward them with `"$@"` (`browser.py:136,239`), so you get, for example, `Lider.AppImage --native-host chrome-extension://…/`. They must never be treated as URLs.
- [ ] GUI mode arguments (`app.py:72-75`, argparse `prog="lider"`, description `"LID: Linux Internet Downloader"`):
  - positional `urls` (`nargs="*"`, help `"addresses to download"`)
  - `--minimized` (help `"start hidden in the system tray"`)
  - `-h/--help` (argparse default, exit 0)
  - Unknown args are passed through to `QApplication` via `parse_known_args` (`app.py:75,80`), for example `-platform offscreen`. Decide what Tauri does with unknown flags.
- [ ] **Single instance** (`app.py:77-78`). The program first calls `send_to_running(urls)`. If an instance answers, it exits 0. With no URLs it sends `--show`.
  - **QUIRK:** `lider --minimized` while LID is already running sends `--show`, so the window pops up. This also happens when autostart runs while LID is open.
- [ ] Start-up order when this process becomes the instance (`app.py:80-129`):
  1. Set up the app: name and display name "LID", desktop file name "lider", `quitOnLastWindowClosed=False` (LID lives in the tray), window icon `app`.
  2. `settings_mod.migrate_legacy_dirs()` (§6).
  3. `settings.load()`.
  4. `desktop.refresh()`, with `OSError` ignored (§5).
  5. Create `Store()`, then `DownloadManager`, then `MainWindow`.
  6. Bind `InstanceServer`. If the bind fails with `OSError` (another instance started at the same moment), retry `send_to_running(urls)`. If that succeeds, exit 0. Otherwise continue with **no server** (`app.py:114-120`).
  7. Show the window unless (`--minimized` or `settings.start_minimized`) **and** a tray icon exists (`app.py:122`). Without a tray, `--minimized` still shows the window.
  8. The first-start "add to menu" offer runs only when the window is shown, through `QTimer.singleShot(0)` (`app.py:124-127`).
  9. Each argv URL is added after the event loop starts with `win.add_url(u, ask_address=False)` (`app.py:128-129`).
  10. On exit: close the server, close the store, return `app.exec()` rc.
- [ ] **Exit codes:**

  | Mode | Code | Meaning |
  |---|---|---|
  | GUI | 0 | URLs or `--show` forwarded to a running instance |
  | GUI | event loop rc | normal run |
  | GUI | 2 | argparse error |
  | Native host | 0 | EOF or short read |
  | Native host | 1 | framing, JSON or UTF-8 error, after an error reply (`native_host.py:139-149`) |
  | Browser-integration CLI | 0 | install or uninstall OK |
  | Browser-integration CLI | 1 | no browser found (`browser.py:258-260`) |
  | `build.sh` | 1 | usage |
  | `build.sh aur` | 2 | `AUR_SOURCE_URL` missing |

- [ ] **Browser-integration CLI output** (`browser.py:251-268`):
  - Uninstall always removes from **all** `BROWSERS`, detected or not. It prints `Removed the LID native messaging host from every browser.` and returns 0. If both flags are given, uninstall wins.
  - Install with nothing detected prints `No supported browser profile found (Chrome, Chromium, Brave, Edge, Vivaldi, Opera, Firefox).` and returns 1. **QUIRK:** LibreWolf is missing from that message.
  - Install prints, in order:
    - `Native messaging host: <exe>`
    - `  registered for <name>` for each browser
    - `Extension for <family>: <folder>` for each family, sorted (exports the extension)
    - `Load that folder as an unpacked extension (see Options -> General -> Browser integration).`
- [ ] **Environment variables:**

  | Variable | Read in | Effect |
  |---|---|---|
  | `LIDER_SOCKET_NAME` | `ipc.py:15` | overrides the socket name; `\0` is always prepended |
  | `LIDER_LAUNCH` | `native_host.py:94-98` | shell-split command the host uses to start LID; written into the wrapper (`browser.py:135`) |
  | `LIDER_NETWORK_TESTS` | `tests/test_media.py:185` | enables the live YouTube test |
  | `APPIMAGE` | `desktop.py:41-42` | AppImage detection; every launch path uses it |
  | `XDG_CONFIG_HOME` | `settings.py:46`, `desktop.py:26`, `browser.py:85` | config folder, autostart folder, Chromium host folders |
  | `XDG_DATA_HOME` | `settings.py:50`, `desktop.py:22` | data folder, menu entry, icon |
  | `XDG_CACHE_HOME` | `settings.py:54`, `build.sh:24` | cache folder; also the build cache |
  | `MOZ_LEGACY_HOME` | `browser.py:101` | forces Firefox and LibreWolf to the legacy `~/.mozilla` side |
  | `QT_QPA_PLATFORM=offscreen` | tests, CI | headless UI tests |
  | `PYTHONNOUSERSITE=1`, `PYTHONHOME`/`PYTHONPATH` unset | `AppRun:4-5` | AppImage start-up |
  | `MAINTAINER`, `PKG_LICENSE`, `PROJECT_URL`, `AUR_SOURCE_URL`, `AMO_JWT_ISSUER`, `AMO_JWT_SECRET` | `build.sh:11-17,27-29` | build settings |

## 2. IPC (single-instance socket)
- [ ] The socket is an **abstract-namespace** Unix `SOCK_STREAM` socket named `"\0" + (LIDER_SOCKET_NAME or f"lider-{uid}")`, for example `\0lider-1000` (`ipc.py:15`). There is no filesystem node. The server uses `listen(8)` and is non-blocking, driven by a `QSocketNotifier` on the GUI loop (`app.py:27-38`).
- [ ] **Client side** (`ipc.py:18-26`):
  - `send_lines(lines)` connects and sends `"\n".join(lines)` (or `"--show"` when the list is empty), plus a trailing `"\n"`, as UTF-8. Then it closes.
  - It returns False when the connect or send fails (no instance running).
  - **There is no handshake and no reply.** Success means only that the connect and send worked.
- [ ] `send_request(dict)` sends a single line, `json.dumps(request, separators=(",",":"))`, which is compact JSON with `ensure_ascii` on by default (`ipc.py:29-30`).
- [ ] `is_running()` only connects, then closes without sending anything (`ipc.py:33-39`). The server reads an empty connection and ignores it.
- [ ] **Server side** (`app.py:40-55`):
  - On accept, it reads until EOF with a 2 s timeout. Any `OSError` keeps whatever data already arrived.
  - It decodes UTF-8 with `errors="replace"`, splits lines with `splitlines()`, strips each line and drops empty ones.
  - Each line is handed to `on_line`.
  - **QUIRK:** the read blocks the GUI loop for up to 2 s per slow client.
- [ ] **Line semantics** (`app.py:101-112`):
  - A line starting with `{` is parsed as JSON. Invalid JSON is ignored silently, and so is any value that is not an object. A valid object goes to `win.handle_browser_request(req)`. **A JSON line does not bring the window forward**, except `action:"show"`.
  - Any other line first calls `win.show_normal()` (`showNormal`, `raise_`, `activateWindow`, `main_window.py:1510-1513`). Then, unless the line is `--show`, it calls `win.add_url(line, ask_address=False)`. Plain lines are not validated as URLs at this point.
- [ ] **JSON requests from the native host to the app.** These are the fields after cleaning in `native_host.py`. Every field except `action` is optional, and each is present only when non-empty.
  - `{"action":"show"}`
  - `{"action":"download","url","final_url","referer","page_url","filename","mime","cookies","user_agent","size"}`
  - `{"action":"links","items":[{"url","text","kind"}],"referer","page_url","cookies","user_agent"}`
- [ ] **How the app uses a request** (`main_window.py:983-1008`):
  - `show` calls `show_normal()`.
  - `cookies` becomes the header `Cookie`, and `user_agent` becomes the header `User-Agent`.
  - `page = page_url or ""` and `referer = referer or page`.
  - `download`: the URL is checked again with `URL_RE`, then `add_url(url, referer=, parent_page=page, ask_address=False, headers=, filename=)`.
  - `filename` is used only when the probe fails, or when the probe's name has no extension and the browser's does (`main_window.py:830-834`). YouTube URLs go to the YouTube flow (`main_window.py:814-816`).
  - `links`: items with a matching URL become `(url, text, kind)`, and `LinksDialog` opens (raised). The chosen links are added with the referer, page and headers.
  - **QUIRK:** `mime`, `size` and `final_url` are forwarded but the UI ignores them.
- [ ] **QUIRK (security):** abstract sockets have no file permissions, so any local user can connect to `\0lider-<uid>` and inject downloads. Consider an `SO_PEERCRED` uid check in Rust.

## 3. Native messaging host (`lider/native_host.py`)
- [ ] **Framing** (`:35-54`):
  - Each message is a 4-byte length, unsigned 32-bit in **native byte order** (`struct "=I"`), followed by UTF-8 JSON.
  - Replies are compact JSON, flushed after every message.
  - `MAX_MESSAGE = 8 MiB` (`:28`).
- [ ] **Read rules:**
  - Fewer than 4 header bytes, or a short body, counts as EOF: exit 0.
  - A size above the limit is a `ValueError("message of {size} bytes is too large")`.
  - Invalid JSON is a `ValueError` (Python's JSON error text).
  - A JSON value that is not an object is `ValueError("a message must be a JSON object")`.
  - Invalid UTF-8 is a `UnicodeDecodeError`.
  - On any of these errors the host writes `{"ok":false,"error":"<msg>"}` and **exits 1** (`:144-146`).
- [ ] **Loop:** read, handle, write, until EOF (`:139-149`). One process serves many messages in sequence. Test `test_host_process_speaks_native_messaging` sends two frames in one stream.
- [ ] **`hello` message** (`:121-125`):
  - It loads the settings from `~/.config/lider/settings.json` directly and **does not start LID**.
  - Reply: `{"ok":true,"version":__version__,"capture":settings.browser_capture,"types":auto_capture_types.split(),"skip_sites":no_auto_sites.split(),"running":ipc.is_running()}`.
  - `split()` splits on any whitespace. The defaults are `browser_capture=True`, `DEFAULT_FILE_TYPES`, and `no_auto_sites=""` (`settings.py:12-16,116-119`).
  - A missing or broken settings file means defaults (`settings.py:208-213`).
  - The Rust port must read the same JSON keys: `browser_capture`, `auto_capture_types`, `no_auto_sites`.
- [ ] **`download` message cleaning** (`_clean_download`, `:57-71`):
  - It starts from `{"action":"download"}` and copies only these string fields, each when non-empty, truncated to a limit:

    | Field | Limit (characters) |
    |---|---|
    | `url` | 8192 |
    | `final_url` | 8192 |
    | `referer` | 8192 |
    | `page_url` | 8192 |
    | `filename` | 1024 |
    | `mime` | 256 |
    | `cookies` | 65536 |
    | `user_agent` | 1024 |

  - `size` is kept when it is an int above 0. **QUIRK:** a JSON `true` passes as 1, because a Python bool is an int.
  - `url` must match `URL_RE = ^(https?|ftp)://\S+$` (case-insensitive, `:29`), or the request is invalid.
  - `final_url`, `referer` and `page_url` are dropped (not rejected) when they do not match. Unknown fields are dropped.
- [ ] **`links` message cleaning** (`_clean_links`, `:74-91`):
  - It takes `items[:5000]` (`MAX_LINKS`) and skips entries that are not objects.
  - Each item needs a matching `url` of at most 8192 characters.
  - `text` must be a string, truncated to 300; otherwise it becomes `""`.
  - `kind` must be one of `a`, `img`, `video`, `audio`; anything else becomes `"a"`.
  - No valid items means an invalid request.
  - It copies `referer`, `page_url`, `cookies` and `user_agent` when they are non-empty strings, truncated to the `TEXT_FIELDS` limits. **QUIRK:** `referer` and `page_url` are *not* URL-validated here, unlike in `download`.
- [ ] **`show` message** becomes `{"action":"show"}`.
- [ ] **Replies** (`:131-136`):
  - `{"ok":true}`
  - `{"ok":false,"error":"invalid request"}`
  - `{"ok":false,"error":"LID is not running and could not be started"}`
  - `{"ok":false,"error":"unknown message type 'x'"}`. This uses Python `repr`: a missing type gives `None`, and a number gives `5`.
- [ ] **Delivery and launch** (`deliver`, `:101-116`):
  1. Try `ipc.send_request`.
  2. If that fails, `Popen(launch_command())` with stdin, stdout and stderr set to `/dev/null` and `start_new_session=True`. If `Popen` raises `OSError`, the result is False.
  3. Then poll: sleep 0.25 s, retry the send, for up to **15 s** (`wait=15.0`).
- [ ] **Launch command** (`:94-98`): `shlex.split($LIDER_LAUNCH)` when set. Otherwise `[sys.executable, "-m", "lider", "--minimized"]`.
- [ ] **Host process requirements:** start fast and use the standard library only (`ipc.py:5`). The host must not touch Qt or the GUI.

## 4. Browser integration (`lider/browser.py`)
- [ ] **Allowed extension IDs, exactly:**
  - Chromium origins: `chrome-extension://hljibhdjbmpphbjhbfgiejnijpeobbne/` (`CHROMIUM_EXTENSION_ID`, fixed by the manifest `key`) and `chrome-extension://ophlhhmpoaonhglimkemkgjifigpghej/` (`EDGE_ADDONS_EXTENSION_ID`, Edge store "CRX ID", Store ID `0RDCKF25DZGT`), in that order (`:29-34`, `listing.md:16`).
  - Firefox: `lid@davidaprilio.github.io` (`:35`).
- [ ] **Host manifest** (`host_manifest`, `:117-123`):
  - Fields: `{"name":HOST_NAME,"description":"LID (Linux Internet Downloader) browser integration","path":<abs exe>,"type":"stdio"}`.
  - Firefox adds `"allowed_extensions":[firefox id]`. Chromium adds `"allowed_origins":[...]`.
  - It is written as `json.dumps(indent=2)` plus `"\n"`, in key order `name, description, path, type, allowed_*`.
  - The file name is `io.github.davidaprilio.lid.json`.
- [ ] **Browser table** (`BROWSERS`, `:70-79`). Detection looks for the profile folder (`detected`, `:143-144`).

  | Name | Family | Profile folder (detection) | User host folder |
  |---|---|---|---|
  | Google Chrome | chromium | `$XDG_CONFIG_HOME/google-chrome` | `.../google-chrome/NativeMessagingHosts` |
  | Chromium | chromium | `$XDG_CONFIG_HOME/chromium` | `.../chromium/NativeMessagingHosts` |
  | Brave | chromium | `$XDG_CONFIG_HOME/BraveSoftware/Brave-Browser` | `.../BraveSoftware/Brave-Browser/NativeMessagingHosts` |
  | Microsoft Edge | chromium | `$XDG_CONFIG_HOME/microsoft-edge` | `.../microsoft-edge/NativeMessagingHosts` |
  | Vivaldi | chromium | `$XDG_CONFIG_HOME/vivaldi` | `.../vivaldi/NativeMessagingHosts` |
  | Opera | chromium | `$XDG_CONFIG_HOME/opera` | `.../opera/NativeMessagingHosts` |
  | Firefox | firefox | `~/.mozilla`, or `$XDG_CONFIG_HOME/mozilla` | `~/.mozilla/native-messaging-hosts`, or `$XDG_CONFIG_HOME/mozilla/native-messaging-hosts` |
  | LibreWolf | firefox | `~/.librewolf`, or `$XDG_CONFIG_HOME/librewolf` | `~/.librewolf/native-messaging-hosts`, or `$XDG_CONFIG_HOME/librewolf/native-messaging-hosts` |

  - `$XDG_CONFIG_HOME` defaults to `~/.config` (`:82-85`). With the test `home` override, `<home>/.config` is used.
  - **Gecko side choice** (`_paths`, `:93-106`): use the legacy home folder when it exists or `MOZ_LEGACY_HOME` is set. Otherwise use the XDG folder (Firefox 147+).
- [ ] **System-wide host folders**, written by the deb and Arch packages (`SYSTEM_HOST_DIRS`, `:110-114`):
  - Chromium family: `/etc/opt/chrome/native-messaging-hosts`, `/etc/chromium/native-messaging-hosts`, `/etc/opt/edge/native-messaging-hosts`, `/etc/opt/brave/native-messaging-hosts`
  - Firefox: `/usr/lib/mozilla/native-messaging-hosts`
  - **QUIRK:** there are no system folders for Vivaldi, Opera or LibreWolf.
- [ ] **User wrapper script** at `$XDG_DATA_HOME/lider/native-host/lider-native-host`, mode 0755 (`:126-140,151-156`). Exact text:
  ```
  #!/bin/sh
  # Native messaging host for the LID browser extension. Written by LID; safe to delete.
  export LIDER_LAUNCH=<shlex.quote(launch_command('--minimized'))>
  <run> "$@"
  ```
  - For an AppImage, `run` is `exec <shlex.quote($APPIMAGE)> --native-host`.
  - Otherwise `run` is `PYTHONPATH=<quoted pkg parent> exec <quoted python> -m lider.native_host`.
  - `launch_command` is `shlex.join(launch_argv()+["--minimized"])` (§5), so the `LIDER_LAUNCH` value is shell-quoted twice.
- [ ] **`install(browsers)`** (`:151-161`) writes the wrapper once, then one manifest per browser, creating folders as needed. **`uninstall`** (`:164-167`) unlinks the manifest. For Gecko browsers it unlinks on **both** sides (legacy and XDG). Missing files are fine. The wrapper itself is not deleted.
- [ ] **`is_installed`** checks that the manifest file exists in the browser's current host folder (`:147-148`).
- [ ] **`export_extension(family, dest, variant="full", update_url="")`** (`:170-189`):
  - It runs `rmtree(dest)`, then copies `browser_extension/`, skipping `manifest.*.json`, `__pycache__` and `*.py`. The `store` variant also skips `youtube.js` and `youtube.css`.
  - It writes `manifest.json` from `manifest.<family>.json` with `version` set to `EXTENSION_VERSION`.
  - The `store` variant removes `key` and `content_scripts` and sets `description` to `STORE_DESCRIPTION` (`"Send downloads and links from the browser to LID (Linux Internet Downloader)."`, `:54`).
  - With an `update_url` for Firefox, it sets `browser_specific_settings.gecko.update_url`.
  - Output is `indent=2` plus `"\n"`.
- [ ] **User extension folders:** `$XDG_DATA_HOME/lider/browser-extension/{chromium,firefox}` (`extension_folder`, `:227-229`). They are exported again each time the user installs.
- [ ] **Reproducible zip** (`zip_folder`, `:192-201`): entries sorted, date 1980-01-01 00:00:00, mode 0644, deflate.
- [ ] **`firefox_updates_json(version, xpi_url)`** returns `{"addons":{"lid@davidaprilio.github.io":{"updates":[{"version":…,"update_link":…}]}}}` (`:204-206`).
- [ ] **`build_extensions(dist, work)`** (`:209-224`) produces:
  - `lid-extension-chromium.zip`: full, with `key`
  - `lid-extension-chromium-store.zip`: store variant
  - `lid-extension-firefox.zip`: listed build, no `update_url`
  - `work/firefox-unlisted/`: with `update_url = FIREFOX_UPDATES_URL`
  - `firefox-updates.json`: `EXTENSION_VERSION`, with link `https://github.com/Davidaprilio/lid/releases/download/v<__version__>/lid-extension-firefox.xpi`
- [ ] **URL constants** (`:39-50`):
  - `RELEASES_URL = https://github.com/Davidaprilio/lid/releases/latest`
  - `FIREFOX_XPI_URL`, `FIREFOX_ZIP_URL`, `FIREFOX_UPDATES_URL` and `CHROMIUM_ZIP_URL` are `RELEASES_URL + "/download/<file>"`
  - `CHROME_WEB_STORE_URL = ""` and `FIREFOX_ADDONS_URL = ""` (not yet published)
  - `FIREFOX_SIGNED_XPI = False`
  - `YOUTUBE_FILES = ("youtube.js","youtube.css")`
- [ ] **`write_system_files(root, host_path, python, pythonpath="")`**, used by the packages (`:232-248`):
  - Host script at `root/host_path`, mode 0755. Exact text:
    ```
    #!/bin/sh
    # Native messaging host for the LID browser extension.
    [PYTHONPATH=<pythonpath> ]exec <python> -m lider.native_host "$@"
    ```
    It has **no `LIDER_LAUNCH`**, so the default `[python, -m, lider, --minimized]` is used.
  - Every `SYSTEM_HOST_DIRS` manifest, mode 0644.
  - Full-variant extension exports at `usr/share/lider/browser-extension/{chromium,firefox}`.
  - **QUIRK (likely bug, verify):** in the deb, the host starts `/usr/bin/python3 -m lider` with only `PYTHONPATH=/usr/lib/lider`. `/usr/lib/lider/vendor` (PySide6) and `/usr/lib/lider/ytdlp` are not on the path, so starting LID from the browser may fail where there is no system PySide6. Compare `build.sh:106-118`.
- [ ] **"Browser integration" dialog**, which uses these constants (`ui/dialogs/browsers.py`):
  - Title "Browser integration". Subtitle "LID talks to its extension through native messaging — no network port is opened."
  - Table columns: check box, `BROWSER`, `LID HELPER`. States: `Installed`, `Not installed`, and `browser not found` (disabled row). Detected browsers are pre-checked, and the header check box toggles all rows.
  - Buttons "Install for checked browsers" and "Remove".
  - Per-family help steps (`STEPS`, `:23-29`) with a read-only folder field (placeholder "Install for a browser first"), plus "Copy" and "Open folder".
  - "Get the extension:" links (`:93-111`), in order:
    - Chrome Web Store, if its URL is set
    - Firefox Add-ons if `FIREFOX_ADDONS_URL` is set; otherwise "Firefox (.xpi)" if `FIREFOX_SIGNED_XPI`; otherwise "Firefox (.zip)"
    - always "All downloads" (`RELEASES_URL`)
  - Each link has the URL as its tooltip.
- [ ] **Other places that show browser status:**
  - Status bar text "Browser: connected" or "Browser: not set up". It is computed once and cached: connected means any detected browser has the host installed (`main_window.py:486-491`). Clicking it opens the dialog (`:454-467`).
  - Options → General has "Capture downloads from browsers (needs the LID extension)" (`settings.browser_capture`), a note, "Set up browsers...", and the pill "✓ N browser(s) connected" or "Not set up yet" (`options.py:328-346`).

## 5. Desktop integration (`lider/desktop.py`)
- [ ] **Paths:**
  - Menu entry: `$XDG_DATA_HOME/applications/lider.desktop` (`:29-30`)
  - Autostart: `$XDG_CONFIG_HOME/autostart/lider.desktop` (`:33-34`)
  - Icon: `$XDG_DATA_HOME/icons/hicolor/scalable/apps/lider.svg`, copied from `lider/ui/assets/icons/app.svg` (`:15,37-38`)
  - System entry: `/usr/share/applications/lider.desktop` (`:16`)
- [ ] **AppImage detection:** `$APPIMAGE` is non-empty (`:41-42`).
- [ ] **`launch_argv()`** (`:45-53`):
  - `[$APPIMAGE]`
  - else `[shutil.which("lider")]`
  - else a **checkout**: `["env", "PYTHONPATH=<pkg parent>", sys.executable, "-m", "lider"]`
  - `launch_command(*extra)` is `shlex.join` of that list plus the extras.
- [ ] **Exec quoting** (`_exec_arg`, `:61-67`, and `_string`, `:70-72`):
  1. Replace `%` with `%%`.
  2. If the arg is non-empty and contains none of space, tab, newline, `"`, `'`, `\`, `>`, `<`, `~`, `|`, `&`, `;`, `$`, `*`, `?`, `#`, `(`, `)` or backtick (`_RESERVED`, `:18`), use it as it is.
  3. Otherwise wrap it in `"…"`, backslash-escaping `"`, backtick, `$` and `\`. Then string-escape the whole value: `\` becomes `\\`, and newline, tab and CR become `\n`, `\t` and `\r`.
  - An empty arg becomes `""`.
  - Expected results (test): `/a/$x` becomes `"/a/\\$x"`, and `/a/"b"` becomes `"/a/\\"b\\""`.
- [ ] **Menu entry text, exact** (`:79-93`):
  ```
  [Desktop Entry]
  Type=Application
  Name=LID
  GenericName=Download Manager
  Comment=Linux Internet Downloader: accelerated multi-connection downloads
  Exec=<quoted argv> %U
  TryExec=<_string(argv[0]) or sys.executable for a checkout>
  Icon=lider
  Terminal=false
  Categories=Network;FileTransfer;Qt;
  MimeType=x-scheme-handler/http;x-scheme-handler/https;x-scheme-handler/ftp;
  X-LID-Written-By=LID
  ```
  `TryExec` hides the entry once the AppImage is deleted.
- [ ] **Autostart text, exact** (`:96-103`). It has **no** marker:
  ```
  [Desktop Entry]
  Type=Application
  Name=LID
  Exec=<quoted argv> --minimized
  Icon=lider
  Terminal=false
  X-GNOME-Autostart-enabled=true
  ```
- [ ] **Helper functions:**
  - `system_installed()`: the system entry exists.
  - `in_menu()`: the user menu entry is a file.
  - `listed_elsewhere()`: an AppImage is running and some *other* `*.desktop` file in the user's applications folder contains the `$APPIMAGE` string. This covers AppImageLauncher, Gear Lever and appimaged (`:115-127`).
- [ ] **`add_to_menu`** copies the icon, writes the entry and runs `update-desktop-database <folder>` when it exists. That call has a 20 s timeout, its output is captured and errors are ignored. **`remove_from_menu`** unlinks the entry and the icon, then runs the same update (`:130-150`).
- [ ] **`set_autostart(on)`** writes or unlinks the autostart file. **`autostart_enabled()`** checks that the file exists (`:153-163`).
- [ ] **`refresh()`**, run at every start (`:166-188`):
  - A checkout (`argv[0]=="env"`) returns `[]` and touches nothing.
  - Rewrite candidates, in order: `menu` (only if the entry exists **and** contains `X-LID-Written-By=LID`), `autostart` (any existing file, marker or not), and `browser helper` (the wrapper, if it exists).
  - If the menu entry is marked **and** a system entry exists, `remove_from_menu()` runs and `"menu"` is reported. The package's entry takes over.
  - Each candidate is rewritten only when the file exists and its text differs from what it should be.
  - It returns the names it rewrote, for example `["menu","autostart","browser helper"]`.
  - Unmarked menu entries, which a menu editor saves, are left alone.
  - It also runs for deb and Arch installs, where `which lider` is found, and re-points autostart and the helper at `/usr/bin/lider`.
- [ ] **First-start prompt** (`ui/dialogs/menu.py`):
  - `should_offer` is true when an AppImage is running and none of these is true: `settings.menu_offered`, `in_menu()`, `system_installed()`, `listed_elsewhere()` (`:37-39`).
  - `offer()` sets `menu_offered=True` and saves the settings **before** asking, so the user is asked once whatever they answer. If they accept, it calls `add_to_menu()` and ignores `OSError` (`:42-50`).
  - Dialog strings (`:15-34`):
    - window title "LID"
    - heading "Add LID to the application menu?"
    - subtitle "`<AppImage basename>` runs on its own, so the application menu does not list LID yet."
    - note "LID keeps the entry pointed at the AppImage you start, also after you move or update it, and it disappears when the AppImage is deleted. You can change this later in Options → General."
    - buttons "Not now" and "Add to menu" (default)
- [ ] **Options → General** (`options.py:310-323,704-713`):
  - "Launch LID on startup" reflects whether the autostart file exists.
  - "Show LID in the application menu" is checked and disabled when the system entry exists, with tooltip "Added by the installed LID package".
  - On accept, only changed items are applied: autostart, then add or remove the menu entry. `OSError` is ignored. `settings.launch_on_startup` is saved.

## 6. Data folders and migration (`lider/core/settings.py`)
- [ ] **Config:** `$XDG_CONFIG_HOME/lider/` holds `settings.json`. It is saved atomically through `settings.tmp` with `indent=2` (`:45-46,216-221`).
- [ ] **Data:** `$XDG_DATA_HOME/lider/` holds `downloads.db` (SQLite), `native-host/lider-native-host` and `browser-extension/{chromium,firefox}` (`:49-50`).
- [ ] **Cache:** `$XDG_CACHE_HOME/lider/`, with `parts/` as the default `temp_dir` (`:53-54,137`).
- [ ] **Legacy names**, newest first: `lidman`, `lindm` (`:57`).
- [ ] **`migrate_legacy_dirs()`** (`:60-87`):
  - For each of config, data and cache, and each legacy sibling folder that exists: if the new folder is missing, rename the old one to it. Otherwise merge: move each item that does not yet exist in the new folder, then `rmdir` the old folder, ignoring failure.
  - Afterwards, in `settings.json`, replace the text `<cache parent>/<legacy>` with the new cache path. This fixes `temp_dir`. Write only if something changed.
- [ ] **Settings loading** ignores unknown keys, falls back to defaults on any error, and has shims (`dark_mode`, `toolbar_large`, and appending new columns). See `:188-213`. This matters because the native host reads the same file.

## 7. Browser extension (`lider/browser_extension/`)
- [ ] **Manifests**: both are MV3, named "LID Integration", short name "LID", description "Send downloads, links and videos from the browser to LID (Linux Internet Downloader).", version 1.0.0, icons 16/32/48/128 PNG, action title "LID" with popup `popup.html` and icons 16/32.
  - Permissions: `downloads`, `contextMenus`, `cookies`, `nativeMessaging`, `storage`, `scripting`. Host permissions: `<all_urls>`.
  - Content script on `https://www.youtube.com/*` and `https://m.youtube.com/*`: `youtube.js` and `youtube.css`, `document_idle`.
  - Chromium only: `key` (the public key that gives the fixed id), `minimum_chrome_version "109"`, `background.service_worker "background.js"` (`manifest.chromium.json`).
  - Firefox only: `background.scripts ["capture.js","background.js"]`, `gecko.id lid@davidaprilio.github.io`, `strict_min_version "140.0"`, `data_collection_permissions.required ["none"]`, `gecko_android.strict_min_version "142.0"` (`manifest.firefox.json`).
- [ ] **Store variant** (§4): no `key`, no `content_scripts`, no `youtube.*` files, the short store description. Because `HAS_YOUTUBE` is false (`background.js:10`), the "Download this video" menu item and `video` messages are disabled.
- [ ] **background.js: messaging and config:**
  - In Chrome it loads `capture.js` with `importScripts` (`:4`).
  - `send()` wraps `sendNativeMessage(HOST, msg)`. It resolves to `{ok:false,error:lastError.message | "no reply from LID" | exception message}` (`:30-41`).
  - `call()` promisifies callback APIs and swallows `lastError` (`:17-28`).
  - The hello config is cached for `CONFIG_TTL = 30000` ms. `getConfig(true)` forces a refresh. A failed reply sets `config=null` and `lastError`, then updates the badge (`:48-56`).
  - `getConfig(true)` runs on `onInstalled`, on `onStartup`, and when the background script loads (`:156-163,250`).
  - The `enabled` flag lives in `chrome.storage.local`. A missing value means enabled (`:43-46`).
- [ ] **Badge and title** (`:58-64`):

  | State | Badge text | Badge colour | Title |
  |---|---|---|---|
  | disabled | `off` | `#475569` | `LID: capture is off` |
  | enabled, no config | `!` | `#B91C1C` | `LID: <lastError or "not connected">` |
  | enabled, connected | empty | `#B91C1C` | `LID` |

- [ ] **Forwarding cookies, referrer and user agent:**
  - `cookieHeader(url)` is `cookies.getAll({url})` joined as `"name=value; name2=value2"` (`:66-69`).
  - `handOver(info)` sends `{type:"download", ...info, cookies, user_agent: navigator.userAgent}`. On failure it sets `lastError`, clears the config and updates the badge (`:71-84`).
- [ ] **Taking over downloads** (`captureDownload`, `:88-110`):
  - It skips items already handled, items not `in_progress`, items started by this extension, and everything while the switch is off.
  - `url = finalUrl || url`, and `name` is the basename of `item.filename`, splitting on `/` or `\`.
  - It calls `LIDCapture.shouldCapture(config, url, name)`.
  - If captured: mark it handled, `pause`, then `handOver({url, referer: item.referrer||"", filename:name, size: totalBytes>0 ? totalBytes : undefined, mime: item.mime||""})`.
  - **Fallback to the browser:** on failure, `resume` and return false.
  - On success, `dropBrowserCopy`: `cancel`, then `search`. If the item is `complete` and `exists!==false`, `removeFile` (Firefox may already have finished). Then `erase` (`:114-121`).
  - Chrome uses `onDeterminingFilename` and holds `suggest()` until LID answers, returning `true` for the async call. Firefox uses `onCreated` (`:128-136`).
  - **QUIRK:** the `handled` set is never cleared until the service worker restarts.
- [ ] **Capture rules** (`capture.js`):
  - `extensionOf`: basename, then strip `?`/`#`, then the text after the last dot only when the dot index is above 0 (dotfiles give `""`), lowercased.
  - `fileNameFromUrl`: `decodeURIComponent` of the last path segment; errors give `""`.
  - Types are shell wildcards matched case-insensitively, `*` meaning `.*` and `?` meaning `.`, anchored at both ends.
  - Skipped sites: lowercased, a leading `*.` is stripped, and the rule matches the host exactly or `*.site` (so `notexample.org` does not match `example.org`).
  - `shouldCapture` is false when there is no config or `capture` is off, when the URL is invalid, or when the protocol is not `http:`, `https:` or `ftp:`. **`blob:` and `data:` therefore stay in the browser.** It is also false for a skipped host.
  - The extension is taken from the browser's file name first, otherwise from the URL. The result is whether it matches the configured types.
  - The module exports to `module.exports` (for the node tests) or `globalThis.LIDCapture`.
- [ ] **Context menus** (`:139-154`). `removeAll` runs first, then the menus are created on install and on startup:

  | id | Title | Contexts | Notes |
  |---|---|---|---|
  | `lid-link` | "Download with LID" | link, image, video, audio | |
  | `lid-selection` | "Download selected links with LID" | selection | |
  | `lid-all` | "Download all links with LID" | page, frame | |
  | `lid-video-page` | "Download this video with LID" | page, video | only with YouTube; `documentUrlPatterns` `*://*.youtube.com/watch*`, `*://*.youtube.com/shorts/*` |

- [ ] **Menu clicks** (`:197-211`). `page = info.pageUrl || tab.url`.
  - `lid-link`: `handOver({url: linkUrl||srcUrl, referer: page, page_url: page})`, which adds cookies for the link URL and the user agent.
  - `lid-video-page`: `send({type:"download", url: page, page_url: page})`, with no cookies or user agent.
  - `lid-all` and `lid-selection`: `scripting.executeScript(collectLinks, [onlySelection])` in `frameIds: [frameId||0]`. When there are items, it sends `{type:"links", items, referer: page, page_url: page, cookies: cookieHeader(page), user_agent}`.
  - **QUIRK:** menu actions ignore the on/off switch and LID's capture setting.
- [ ] **`collectLinks`**, which runs inside the page (`:166-186`):
  - It collects `a[href]` (href and textContent, kind `a`), `img[src]` (currentSrc or src, alt, kind `img`), and `video, audio` (currentSrc or src, title) together with their child `source[src]`.
  - URLs are deduplicated and must match `/^(https?|ftp):/i`.
  - In selection mode an element must intersect one of the selection ranges.
  - Text is trimmed, runs of whitespace collapse to one space, and it is cut to 300 characters. At most 5000 items.
- [ ] **Runtime messages** from the popup and content script (`:215-248`). The sender is not checked.
  - `{type:"video",url}` (YouTube builds only) goes through `sendVideo`. In-flight requests are deduplicated per URL with a Map, and the message sent is `{type:"download",url,page_url:url}`. The native reply is passed back.
  - `{type:"status"}` forces `getConfig(true)` and replies `{connected,enabled,version,capture,running,error:lastError}`.
  - `{type:"set-enabled",enabled}` stores the flag, updates the badge and replies `{ok:true}`.
  - `{type:"show"}` sends `{type:"show"}` to the host and passes the reply back.
- [ ] **Popup** (`popup.html`, `popup.js`, `popup.css`):
  - Header: icon 32 shown at 24 px, "LID", "Linux Internet Downloader".
  - `#status` starts as "Checking…". There is a switch "Take over browser downloads", a `#hint`, a button "Open LID", and a hidden secondary button "Get LID".
  - **Connected:** "Connected to LID `<version>`" with class `ok`. Hint "Downloads of the file types set in LID go to LID.", or when capture is off, "Capture is off in LID: Options → General → Browser integration."
  - **Not connected:** hide "Open LID", show "Get", class `bad`, then `getPlatformInfo`:
    - Not Linux: "LID runs on Linux only". Hint "This extension hands downloads to LID, a download manager for Linux. There is no LID on this system, so downloads stay in the browser." The switch is hidden, the button becomes "About LID", and it links to the project home.
    - Linux: "LID helper not found". Hint "This extension works with LID, a download manager for Linux. Install LID, then in LID open Options → General → Set up browsers… and install the helper for this browser." The button links to releases/latest.
  - Changing the switch sends `set-enabled`. "Get" opens a tab, then closes the popup. "Open LID" sends `show`, then closes the popup.
  - CSS: 280 px wide, light and dark tokens through `prefers-color-scheme`.
- [ ] **YouTube button** (`youtube.js`, `youtube.css`):
  - It works on video pages: path `/watch` or starting with `/shorts/`. On other pages the button is removed.
  - **Watch page on www:** it waits for `ytd-watch-metadata ytd-menu-renderer`, retrying every 500 ms up to `WAIT_TRIES=20` (10 s), then falls back to the floating button.
  - **Template choice:** a visible `:scope > div button` with an SVG icon part, not inside an element whose tag matches `/like/i`, not inside `#lid-download`. One with a text label is preferred.
  - **Clone:** a wrapper `div#lid-download` (`display:flex`, `flex:none`, margins copied from the template's row item) around a deep clone of the template. The clone loses `id`, `title`, `aria-pressed`, `aria-expanded`, `aria-haspopup` and `disabled`. It gets `aria-disabled="false"`, `aria-label="Download with LID"`, title "Download with LID: choose the resolution or audio only", and `type=button`. Its SVG is replaced by the LID logo: an arrow path `M12 3.5v9.5M8 9l4 4 4-4` over three bars at x 3.5, 9.75 and 16 with opacity 1, 0.7 and 0.45.
  - **Insertion:** the label text becomes "LID". The wrapper goes before `:scope > #flexible-item-buttons`, or else after the row.
  - **Re-render handling:** a MutationObserver (childList) on the menu calls `refresh()` when the wrapper is detached. It is not rebuilt when it already sits in the menu with the same class name as the template.
  - **Floating fallback** (Shorts, m.youtube.com, unknown layouts): `button#lid-download.lid-floating` with the logo and label "Download with LID", appended to `<html>`. It is fixed at bottom-right 24 px, z-index 2147483000, pill shape, `#1d4ed8`, hover `#1e40af`, with state colours busy `#475569`, ok `#0f766e`, error `#b91c1c`.
  - **Click states** (`root.dataset.state`): idle, then busy "Sending…", then "Sent to LID" or "LID not found", back to idle after 2500 ms. Clicks are ignored unless idle. Icon-only clones show the text in the title, with colour ok `#2ba640`, error `#e5484d`, and opacity 0.6 while busy. The click sends `{type:"video", url: location.href}`.
  - **Navigation:** `yt-navigate-finish` and `popstate` call `refresh()`, which resets the retry count and removes a cloned (non-floating) button.

## 8. Desktop file, packaging and release
- [ ] **Repo `lider.desktop`:**
  - Keys: Type Application, Name LID, GenericName Download Manager, Comment "Linux Internet Downloader: accelerated multi-connection downloads", `X-GNOME-FullName=Linux Internet Downloader`, `Exec=lider %U`, Icon lider, Terminal false, `Categories=Network;FileTransfer;Qt;`, MimeType http/https/ftp scheme handlers.
  - It has no marker. The port must keep the handlers for the http, https and ftp schemes.
- [ ] **`pyproject.toml`:**
  - Python 3.10 or newer, requires `PySide6>=6.5`, optional extra `youtube = yt-dlp>=2026.8.19`.
  - Script `lider = lider.app:main`.
  - Package data: icons SVG, `browser_extension/*`, `browser_extension/icons/*`.
- [ ] **`build.sh` overview** (`packaging/build.sh`):
  - Targets: `appimage`, `deb`, `arch`, `aur`, `extension`, `sdist`, `all` (all builds appimage, deb, arch and extension). No argument prints usage from lines 2-17 and exits 1 (`:279-288`).
  - Defaults: `MAINTAINER="David Aprilio <david1.aprilio4@gmail.com>"`, `PKG_LICENSE=MIT`, `PROJECT_URL=https://github.com/Davidaprilio/lid`.
  - `SUMMARY="Linux Internet Downloader (LID): fast multi-connection downloads"`.
  - Pinned versions: Python 3.12, `PySide6-Essentials==6.11.1`, `yt-dlp==2026.8.19`, `web-ext@10.7.0`, appimagetool 1.9.0.
  - Work folder `build/packaging`. Cache `~/.cache/lider-build`.
- [ ] **sdist** (`:44-54`): `dist/lider-<v>.tar.gz` holding `lider/` (without `__pycache__`), `pyproject.toml`, `README.md`, `LICENSE` and `lider.desktop`. It is reproducible: `--sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner`. It does not contain `packaging/`.
- [ ] **`install_tree`** (`:57-68`) lays out: app under `<pydir>/lider` (dirs 755, files 644), `/usr/bin/lider` launcher, `/usr/share/applications/lider.desktop`, `/usr/share/icons/hicolor/scalable/apps/lider.svg`, `/usr/share/doc/lider/README.md`.
- [ ] **deb** (`:96-160`). Output `dist/lider_<v>-1_amd64.deb`.
  - App in `/usr/lib/lider/lider`. PySide6 abi3 wheels for cp310, `manylinux_2_34_x86_64`, in `/usr/lib/lider/vendor`. yt-dlp in `/usr/lib/lider/ytdlp`, installed with `--no-deps`. The `bin/` folders are removed.
  - `/usr/bin/lider` is a Python script that puts `/usr/lib/lider` and `vendor` first on the path, and puts `ytdlp` right after the user site-packages, so a `pip install --user` yt-dlp wins (`:106-118`).
  - Browser files come from `write_system_files(root, /usr/lib/lider/lider-native-host, /usr/bin/python3, /usr/lib/lider)`. PySide6 is trimmed.
  - Licences: third-party licences go to `/usr/share/doc/lider/third-party/<pkg>/`. The texts in `packaging/licenses/` (`GPL-3.0-only.txt`, `LGPL-3.0-only.txt`, `PySide6-NOTICE.txt`) go to `third-party/PySide6/`. `/usr/share/doc/lider/copyright` is in DEP-5 format.
  - `control.in` fields: Package lider, Version `<v>-1`, Section net, Priority optional, Architecture amd64, Installed-Size from `du -sk`, the Depends list (`:96-99`), `Recommends: ffmpeg, nodejs | deno`, Homepage, and a long description.
  - `DEBIAN/md5sums` covers `usr/`. `DEBIAN/conffiles` lists every file under `/etc`, which are the host manifests.
  - The package is assembled with `ar rcD` over `debian-binary` ("2.0"), `control.tar.xz` and `data.tar.xz` (`./usr` and `./etc`), using reproducible tar flags.
  - **QUIRK:** the Homepage-deletion line (`:145`) can never run, because `PROJECT_URL` always has its default.
- [ ] **postinst and prerm:**
  - `postinst configure` runs `python3 -m compileall -q /usr/lib/lider/lider /usr/lib/lider/ytdlp || true`.
  - `prerm` deletes every `__pycache__` under `/usr/lib/lider`.
- [ ] **Arch** (`:163-181`, `arch/PKGBUILD.in`):
  - `build_arch` builds the sdist, fills in the template (`@VERSION@ @MAINTAINER@ @LICENSE@ @SUMMARY@ @URL@ @SOURCE@ @SHA256@`), and runs `makepkg -f --nodeps --noconfirm` with `PKGDEST=dist`. Output `lider-<v>-1-any.pkg.tar.zst`.
  - PKGBUILD:
    - `arch=any`
    - depends `python pyside6 qt6-svg`
    - optdepends:
      - yt-dlp: download YouTube videos
      - ffmpeg: join YouTube video and audio, convert to MP3
      - nodejs: unlock YouTube streams (or deno or bun)
      - deno: unlock YouTube streams
    - The package goes into the system site-packages, byte-compiled with `-s $pkgdir`.
    - `write_system_files($pkgdir, /usr/lib/lider/lider-native-host, /usr/bin/python)`, without a PYTHONPATH.
    - `/usr/bin/lider` launcher, desktop file, icon, README, and the licence at `/usr/share/licenses/lider/LICENSE`.
- [ ] **AUR** (`:183-198`):
  - Requires `AUR_SOURCE_URL`; otherwise it prints an example and exits 2.
  - Writes `dist/aur/PKGBUILD` with `source=lider-$pkgver.tar.gz::<url>` and the sha256 of the local sdist, then runs `makepkg --printsrcinfo > .SRCINFO`.
- [ ] **Sibling `../aur-lider/`** (a separate AUR checkout next to this repository):
  - Contents: `PKGBUILD`, `.SRCINFO`, and a `.gitignore` that ignores everything except `PKGBUILD`, `.SRCINFO` and `.gitignore`. There is one commit, `3a6e761 lider 1.0-1: initial upload`.
  - **Stale:** it is at version 1.0, with source `.../download/v1.0/lider-1.0.tar.gz` and sha256 `93cc5ffd…8e9576`. Its `package()` has **no** `write_system_files` step, so 1.0 has no system host manifests. Otherwise it matches the template.
  - Publishing: clone `ssh://aur@aur.archlinux.org/lider.git` (`packaging/README.md:100-106`).
- [ ] **Extension build** (`:201-221`):
  - It removes old `dist/lid-extension-*` and `firefox-updates.json`, then runs `build_extensions`.
  - It runs `npx web-ext@10.7.0 lint --source-dir work/firefox --self-hosted` and keeps the last 15 lines of output.
  - When `AMO_JWT_ISSUER` and `AMO_JWT_SECRET` are set, it runs `web-ext sign --channel unlisted` on `firefox-unlisted` and copies the result to `dist/lid-extension-firefox.xpi`.
  - Without the keys it **deletes `firefox-updates.json`**.
  - Store-submission rule: `key.pem` must never be committed. On the first Chrome Web Store upload it can be zipped in to keep the id. Otherwise add the store id to `CHROMIUM_EXTENSION_IDS`, and the id must ship in a LID release before submission (`packaging/README.md:157-169`, `listing.md:6-20`).
- [ ] **AppImage** (`:224-277`). Output `dist/Lider-<v>-x86_64.AppImage`.
  - Python 3.12 from python-build-standalone via `uv` (`UV_PYTHON_INSTALL_DIR=cache`), copied to `AppDir/usr/python`, with `EXTERNALLY-MANAGED` removed.
  - pip installs PySide6-Essentials and yt-dlp, and `lider/` is copied into site-packages.
  - Trimmed: test, idlelib, tkinter, turtledemo, ensurepip, the Tcl/Tk libraries, include, the PySide6 QML and qtwebengine translations, and the designer tools. pip is uninstalled and the code is byte-compiled.
  - Licences go to `usr/share/licenses/lider/`: `LICENSE`, `third-party/python/LICENSE.txt`, the dist-info licences, and `third-party/PySide6/*`.
  - AppDir layout: `AppRun`, a root `lider.desktop` (with `Exec=lider %U`), `lider.svg`, `.DirIcon` linking to `lider.svg`, and the usr share desktop file and icon.
  - Packed with `ARCH=x86_64 appimagetool --appimage-extract-and-run --no-appstream`.
  - `AppRun`: `HERE=$(dirname $(readlink -f $0))`, `export PYTHONNOUSERSITE=1`, `unset PYTHONHOME PYTHONPATH`, `exec $HERE/usr/python/bin/python3 -m lider "$@"`.
- [ ] **Licence files** in `packaging/licenses/`: `GPL-3.0-only.txt`, `LGPL-3.0-only.txt`, `PySide6-NOTICE.txt`. With Rust and Tauri these Qt/PySide notices go away; replace them with notices for the Rust crates and WebKitGTK.
- [ ] **Release workflow** (`.github/workflows/release.yml`):
  - Triggers: a `v*` tag push, or `workflow_dispatch` (builds only, no release). Permission `contents: write`.
  - **`test`** (ubuntu-24.04, Python 3.12):
    - apt installs `libegl1 libgl1 libxkbcommon0 libfontconfig1 libdbus-1-3 ffmpeg`; pip installs PySide6-Essentials 6.11.1 and yt-dlp 2026.8.19.
    - On tags, it **checks that the tag minus `v` equals `__version__`**, or fails with `::error::`.
    - Runs `python -m unittest discover -s tests -t . -v` with `QT_QPA_PLATFORM=offscreen`, then `node --test tests/js/capture.test.cjs`.
  - **`appimage`** (needs test): `desktop-file-utils`, `uv==0.12.17`, `build.sh appimage`. Artifact `appimage`: `dist/*.AppImage`.
  - **`deb`**: `build.sh deb`. Artifact `deb`: `dist/*.deb`.
  - **`arch`**:
    - Runs in the `archlinux:latest` container with `pacman -Syu base-devel git python sudo`. It creates a `builder` user, then runs `build.sh arch` and `build.sh aur` with `AUR_SOURCE_URL=https://github.com/$GITHUB_REPOSITORY/releases/download/v<v>/lider-<v>.tar.gz`.
    - It packs `dist/lider-<v>-aur.tar.gz` from `PKGBUILD` and `.SRCINFO`.
    - Artifact `arch`: `dist/*.pkg.tar.zst` and `dist/lider-*.tar.gz` (the sdist plus the aur tarball).
  - **`extension`** (Node 22):
    - `AMO_JWT_*` secrets are passed only on tag refs, otherwise empty, because AMO signs each version once.
    - Runs `build.sh extension`. Artifact `extension`: `dist/lid-extension-*` and `dist/firefox-updates.json`.
  - **`release`** (needs all four jobs, tag only):
    - Downloads every artifact merged into `dist/`, then runs `sha256sum * > SHA256SUMS`.
    - If a release for the tag exists (a draft or a moved tag), `gh release upload --clobber`. Otherwise `gh release create <tag> dist/* --title "LID <v>" --generate-notes`.
  - **Final release files:**
    - `Lider-<v>-x86_64.AppImage`
    - `lider_<v>-1_amd64.deb`
    - `lider-<v>-1-any.pkg.tar.zst`
    - `lider-<v>.tar.gz`
    - `lider-<v>-aur.tar.gz`
    - `lid-extension-chromium.zip`, `lid-extension-chromium-store.zip`, `lid-extension-firefox.zip`
    - `[lid-extension-firefox.xpi, firefox-updates.json]`, only when signed
    - `SHA256SUMS`
  - The links in `browser.py`, `popup.js` and the Firefox `update_url` all depend on these exact file names under `releases/latest/download/`.
- [ ] **`.gitignore`:** `__pycache__/`, `*.pyc`, `/build/`, `/dist/`, `*.egg-info/`, `.venv/`. `CLAUDE.md` is kept out through `.git/info/exclude`. `dist/` currently holds 1.0 and 1.1 builds and the extension zips.
- [ ] **Edge store material** (`packaging/store/edge/`):
  - `listing.md` holds the submission sheet: permission justifications, description, and certification steps that quote these UI strings: "Options → General → Set up browsers…", "Install for checked browsers", "Connected to LID `<VERSION>`", the "Download File Info" window, "Take over browser downloads", and the user manifest path `~/.config/microsoft-edge/NativeMessagingHosts/io.github.davidaprilio.lid.json`.
  - `privacy.md` states that the data stays local in `~/.local/share/lider` and that the host is removed through Options → General → Set up browsers… → Remove.
  - The privacy URL points at the `redesign` branch.
  - `assets/` has the logo (300x300), small and large tiles, and three screenshots.
  - The port must keep these strings and paths, or the listing and the privacy policy have to change with it.

## 9. Tests in scope (one line each)
**`tests/test_app.py`**
- [ ] `test_nobody_listening`: `send_to_running` returns False when no server is on the name.
- [ ] `test_lines_reach_the_running_instance`: two URLs, then `[]`, arrive as `["https://a/1.zip","https://b/2.zip","--show"]`. A second server on the same name raises `OSError`. After close, the send fails.

**`tests/test_browser.py`**
- [ ] `test_chromium_key_gives_the_registered_extension_id`: sha256(base64-decoded key)[:32], hex mapped to a-p, equals `CHROMIUM_EXTENSION_ID`.
- [ ] `test_firefox_id_matches`: the gecko id in the Firefox manifest equals `FIREFOX_EXTENSION_ID`.
- [ ] `test_manifests_reference_existing_files`: every icon, popup, background and content-script file in both manifests exists, and `nativeMessaging` is present.
- [ ] `test_background_uses_the_same_host_name`: `background.js` contains `"io.github.davidaprilio.lid"`.
- [ ] `test_store_variant_has_no_youtube_and_no_key`: the full build has `key` and `content_scripts`. The store build has neither, no `youtube.js`, version equal to `EXTENSION_VERSION`, and no "YouTube" in the description.
- [ ] `test_update_url_only_for_the_self_distributed_firefox_build`: the listed build has no `update_url`, and the unlisted build has `FIREFOX_UPDATES_URL`.
- [ ] `test_release_files_are_reproducible`: two builds are byte-identical and produce exactly 4 files. The store zip has `manifest.json` but no `youtube.js`. The updates JSON has `EXTENSION_VERSION` and a link ending `/v<__version__>/lid-extension-firefox.xpi`.
- [ ] `test_host_accepts_every_known_chromium_id`: `allowed_origins` lists every id in `CHROMIUM_EXTENSION_IDS`, in order, as `chrome-extension://<id>/`.
- [ ] `test_detect_install_uninstall`: with Brave and `~/.mozilla` present, detection gives `["Brave","Firefox"]`. The wrapper is executable and runs `lider.native_host`. The Brave manifest has both Chromium origins and the path. The Firefox manifest has `allowed_extensions`. `is_installed` goes true, then false after uninstall.
- [ ] `test_firefox_in_the_xdg_config_folder`: without `~/.mozilla` but with `~/.config/mozilla/firefox`, the manifest goes to `~/.config/mozilla/native-messaging-hosts`, `~/.mozilla` is not created, and install and uninstall both work.
- [ ] `test_appimage_wrapper_runs_the_appimage`: with `APPIMAGE="/opt/apps/Lider 1.0.AppImage"`, the wrapper contains `exec '/opt/apps/Lider 1.0.AppImage' --native-host`.
- [ ] `test_export_extension_per_family`: the Firefox export has the gecko id in `manifest.json`, no `manifest.*.json` files, and `background.js`.
- [ ] `test_system_files_for_packages`: the host script contains `PYTHONPATH=/usr/lib/lider exec /usr/bin/python3 -m lider.native_host`. The Chrome system manifest path is `/usr/lib/lider/lider-native-host`. The Mozilla system manifest exists, and so does `usr/share/lider/browser-extension/chromium/manifest.json`.

**`tests/test_desktop.py`** (`APPIMAGE="/opt/apps/Lider 1.2.0.AppImage"`, XDG folders in a temp dir, `SYSTEM_ENTRY` and `_update_menus` patched)
- [ ] `test_exec_quoting`: plain paths stay as they are, spaces get double quotes, `$` and `"` become `\\$` and `\\"` inside quotes, `%` becomes `%%`.
- [ ] `test_menu_entry_runs_the_appimage`: `Exec="<appimage>" %U`, `TryExec=<appimage>` unquoted, `Icon=lider`, and autostart `Exec="<appimage>" --minimized`.
- [ ] `test_entry_passes_desktop_file_validate`: `desktop-file-validate` reports no "error" (skipped when the tool is missing).
- [ ] `test_add_and_remove`: add creates the entry at `data/applications/lider.desktop` and an icon identical to `app.svg`. Remove deletes both.
- [ ] `test_autostart`: on and off toggles `config/autostart/lider.desktop`.
- [ ] `test_refresh_follows_a_moved_appimage`: no change gives `[]`. After `APPIMAGE` moves, the result is `["menu","autostart","browser helper"]`, the new Exec is unquoted, the wrapper has `exec <new> --native-host`, and `listed_elsewhere` is False.
- [ ] `test_refresh_leaves_missing_files_alone`: `refresh()` creates nothing.
- [ ] `test_a_checkout_does_not_take_over_the_entries`: with no `APPIMAGE` and no `lider` on PATH, `argv[0]=="env"`, `refresh()==[]`, and the entry is unchanged.
- [ ] `test_an_entry_from_a_menu_editor_is_left_alone`: an unmarked entry is not rewritten.
- [ ] `test_an_old_autostart_entry_is_fixed`: an old autostart with `python3 -m lider` is rewritten (`["autostart"]`) to the AppImage Exec.
- [ ] `test_package_entry_takes_over`: when the system entry exists, refresh reports "menu" and removes the user entry.
- [ ] `test_offer_once_and_only_to_an_appimage`: `should_offer` is true on a fresh start and false after `menu_offered`, when another `.desktop` file has the AppImage path, or without `APPIMAGE`.
- [ ] `test_options_general_page`: the menu checkbox starts enabled and unchecked. Accepting writes the menu entry and the autostart, and sets `launch_on_startup`. With a system entry, the checkbox is disabled and checked, and launch is checked.

**`tests/test_native_host.py`**
- [ ] `test_round_trip`: write, then read, a message with non-ASCII keys.
- [ ] `test_end_of_input`: empty input reads as None.
- [ ] `test_oversized_message_is_refused`: a 64 MiB header raises `ValueError`.
- [ ] `test_hello_reports_the_capture_settings`: the saved settings `capture=False`, `types="ZIP ISO"` and `sites="example.com"` come back as `(True, False, ["ZIP","ISO"], ["example.com"], running False)`.
- [ ] `test_download_is_forwarded_with_only_known_fields`: unknown fields are dropped and the `javascript:` `page_url` is removed. The forwarded line is exactly `{action,url,referer,cookies,user_agent,size}` and the reply is `{"ok":true}`.
- [ ] `test_bad_urls_are_rejected`: `javascript:`, `file:`, empty and null URLs give `ok:false`. A links request with only a `data:` item gives `"invalid request"`.
- [ ] `test_links_are_cleaned`: an unknown kind becomes "a", a missing text becomes "", `chrome://` and non-dict items are dropped, and `page_url` is kept.
- [ ] `test_starts_lid_when_it_is_not_running`: with `Popen` patched to start a fake server, `deliver` retries and the `{"action":"show"}` request arrives.
- [ ] `test_unknown_type`: `format-disk` gives `ok:false`.
- [ ] `test_host_process_speaks_native_messaging`: a `python -m lider.native_host` subprocess with two frames gives a hello `ok` with `ZIP` in types, then an unknown type `ok:false`. Exit code 0, and no PySide6 in stderr.
- [ ] `test_native_host_does_not_load_qt`: importing `lider.native_host` loads no PySide6 modules.
- [ ] `test_send_request_and_is_running`: `is_running` is false, then true with the fake server, and `send_request` returns True.

**`tests/js/capture.test.cjs`** (config types `ZIP ISO R0* MP4`, skip `example.org`)
- [ ] "file extension from names and URLs": `.ISO` gives `iso`, `a.tar.gz` gives `gz`, `.bashrc` gives `""`, and `My%20File.zip?x=1` gives `My File.zip`.
- [ ] "captures listed types, wildcards included": `a.zip` from the URL, `mp4` from the browser's file name when the URL has none, `part.r01` through `R0*`, and `.html` is not captured.
- [ ] "respects the switch, schemes and excluded sites": `capture:false`, a null config, `blob:`, and `dl.example.org` are not captured, while `notexample.org` is.

**Other suites** (not in scope; port or replace them): `test_engine`, `test_manager`, `test_media` (the live test needs `LIDER_NETWORK_TESTS=1`, and media tests skip without ffmpeg), `test_search`, `test_ui`, and the fixture `tests/httpfixture.py`.

## 10. Quirks to decide on before the port
1. `--minimized` sent to a running instance shows the window, through `--show` (§1).
2. The IPC socket has no peer-uid check, which matters because abstract sockets have no permissions (§2).
3. The IPC read blocks the GUI loop for up to 2 s per slow client (§2).
4. `mime`, `size` and `final_url` go through the host but the UI ignores them (§2).
5. Host `size` accepts boolean `true` as 1, and links do not URL-validate `referer`/`page_url` (§3).
6. Error texts are Python-specific: `repr` in "unknown message type", and the JSON decoder's messages (§3).
7. The deb host's auto-launch path is likely missing `/usr/lib/lider/vendor` (§4). With Rust the PYTHONPATH wrappers can go, but the host manifests' `path` and the `LIDER_LAUNCH` mechanism must still work.
8. There are no system host folders for Vivaldi, Opera or LibreWolf, and the CLI's "no browser" message leaves out LibreWolf (§1, §4).
9. Extension context-menu actions ignore the on/off switch, and the `handled` set grows without limit (§7).
10. The `../aur-lider` repo is stale at 1.0 and lacks the browser-integration install step (§8).
11. The deb Homepage-removal line can never run (§8).
