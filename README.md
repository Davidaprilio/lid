# 🔥 LID — Linux Internet Downloader

LID is a fast download manager for Linux. It splits every download over several connections,
resumes where it stopped, sorts files into folders, runs queues on a schedule, and downloads
YouTube videos in the resolution you pick. It is written in Python and Qt (PySide6).

Package and command name: **`lider`**.

## Features

* **Multi-connection downloads** (1–32 connections, default 8) with dynamic segmentation:
  when a connection finishes, it splits the largest remaining part and keeps going.
* **Pause and resume.** The state survives restarts. A server that ignores `Range` falls back to one connection.
* **Categories** (General, Compressed, Documents, Music, Programs, Video) chosen by file type or site,
  each with its own folder.
* **Queues and Scheduler.**
  * Main and Synchronization queues, plus any queues you add.
  * Start at a time, once or on chosen weekdays; stop at a time.
  * Start on startup, periodic synchronization, retries, N files at the same time.
  * When done: open a file, exit, or power off.
* **Dialogs:** Download File Info, Download progress (status / speed limiter / options on completion,
  per-connection bar, list and speed), Download complete and Properties.
* **Search:** SearchBar (Ctrl+F) that filters the list, and Find Next (F3).
  You choose the targets and set Match case / Match whole string.
* **YouTube:** paste a YouTube link (watch, youtu.be, shorts, live) and LID detects it.
  * Download File Info then shows a "YouTube video" box. Pick Video with a resolution (144p–2160p),
    or Audio only (M4A, MP3, Opus).
  * yt-dlp only reads the page. LID's multi-connection engine fetches the streams (about 11× faster
    than one connection in testing), and ffmpeg joins them.
  * You need `pip install yt-dlp` (or `pip install -e .[youtube]`), ffmpeg, and a JavaScript runtime
    (deno, node or bun) for YouTube's stream protection.
* **Browser integration** (Chrome, Chromium, Brave, Edge, Vivaldi, Opera, Firefox) with the LID extension:
  * downloads of your file types are handed to LID with their cookies, referrer and user agent,
    so downloads behind a login keep working;
  * "Download with LID", "Download all links with LID" and "Download selected links with LID" in the
    right-click menu;
  * a "LID" button in YouTube's own action bar (next to Share), cloned from YouTube's buttons so it
    matches their current style and theme.
* **Speed limiter**, global and per file.
* **Adding downloads:** batch download with `*` wildcards, a one-page Grabber, and "batch from clipboard".
* **Clipboard monitoring** for the auto-capture file types, plus drag-and-drop.
  A single instance takes URLs from the command line.
* **Import / export** of `.ef2` export files and plain text lists.
* **Themes:** Ember (light), Ember Dark and Dark. LID follows the system light/dark setting
  unless you pick one in View → Theme or Options → Appearance.
* **Download list:** check boxes with a check-all header. Start, Stop and Delete act on the checked
  downloads (or the selected ones), and the status column shows progress pills.
* **Checksum verification** (optional): paste an MD5, SHA-1, SHA-256 or SHA-512 value — or a
  SHA256SUMS line — in Download File Info or File Properties. When the download finishes, LID reads
  the file and marks it Verified or Checksum mismatch; on a mismatch it asks, keeps the file or
  downloads it again (Options → Downloads).
  Right-click → Check hash… checks a finished file against a pasted value (or just computes its
  hash to copy), or, for a download still in progress, checks it as soon as it finishes.
* **Details pane** (F9) under the list: progress, the connections bar and a per-download speed limit
  of the highlighted download, with a button to open it in its own window.
* **View:** tray icon, list density, toolbar labels, a font picker and column customisation;
  View → Appearance... opens the Appearance options. The sidebar can be resized and keeps its width.

## Install

Download a package from the [Releases page](https://github.com/Davidaprilio/lid/releases), or build one with
`./packaging/build.sh` (see [packaging/README.md](packaging/README.md)).

```sh
sudo pacman -U lider-1.0-1-any.pkg.tar.zst      # Arch, CachyOS, Manjaro, EndeavourOS
sudo apt install ./lider_1.0-1_amd64.deb         # Debian 12+, Ubuntu 22.04+
chmod +x Lider-1.0-x86_64.AppImage && ./Lider-1.0-x86_64.AppImage   # any distro
```

## Browser integration

LID talks to its browser extension through native messaging, so no network port is opened.

1. In LID open **Options → General → Set up browsers…** and click **Install for checked browsers**.
   This registers the LID helper with each browser found. From a terminal you can run
   `lider --install-browser-integration` instead. The .deb and Arch packages register it system-wide.
2. Add the extension to the browser. The dialog's *Get the extension* buttons lead to the downloads.
   * **Firefox:** open `lid-extension-firefox.xpi` from the
     [latest release](https://github.com/Davidaprilio/lid/releases/latest). It is signed by Mozilla
     and updates itself.
   * **Chrome, Chromium, Brave, Edge, Vivaldi, Opera:** unzip `lid-extension-chromium.zip` from the
     latest release, or use the folder the dialog shows (the packages also put it in
     `/usr/share/lider/browser-extension/`). Open `chrome://extensions`, turn on *Developer mode*,
     then *Load unpacked*.

   The full extension adds a "LID" button to the action bar of YouTube watch pages. Store listings,
   once published, leave that button out because of store rules; pasting a YouTube link into LID always
   works.

The toolbar button shows whether LID is reachable and turns capturing on or off. Downloads LID
cannot take (blob: links, LID not available) stay in the browser.

## Run from source

```sh
python3 -m lider                 # open the window
python3 -m lider URL [URL ...]   # add downloads (forwards to a running instance)
python3 -m lider --minimized     # start in the tray
```

`pip install -e .` installs the `lider` command.

The icons are LID's own "Signal" set (SVG in `lider/ui/assets/icons/`), and they recolour
themselves for dark mode.

LID stores its files here:
* settings: `~/.config/lider/settings.json`
* download list: `~/.local/share/lider/downloads.db`
* partial files: `~/.cache/lider/parts`

## Tests

```sh
QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tests -t .
```

The engine tests download from a local HTTP server and compare SHA-256 hashes. That server
supports ranges and can ignore them, send no length, drop connections, refuse requests and throttle.

## Layout

```
lider/core/   engine.py (segmented HTTP), manager.py (queues, scheduler), models.py, store.py (SQLite),
              settings.py (defaults), categories.py, search.py, fmt.py,
              media.py + media_task.py (YouTube variants, stream download, ffmpeg join)
lider/ui/     main_window.py, download_model.py, dialogs/*.py, icons.py + assets/icons/*.svg
packaging/    build.sh for AppImage, .deb, Arch and AUR
tests/        unit, engine, YouTube and offscreen UI tests
```

## License

MIT, see [LICENSE](LICENSE).

LID is an independent project. It is not affiliated with, endorsed by or connected to Tonec Inc.
or Internet Download Manager. YouTube is a trademark of Google LLC. Download only content you
have the right to download.
