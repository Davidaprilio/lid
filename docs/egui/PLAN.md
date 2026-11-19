# LID 2.0: the window in egui

The engine, manager, view model (`lider-core`) and the OS ports (`lider-sys`) stay as they are. Only the
window changes: from a WebView (Tauri + Svelte, `src-tauri/`, `ui/`) to egui, drawn by LID itself. No
WebKitGTK/WebView2, one language, a much smaller AppImage, less memory, dialogs that open at once.

Branch `egui`, cut from `ports`. `src-tauri/` and `ui/` stay in the tree until the egui window has
every feature (same rule as the Python code during the Tauri port); the beta keeps building from them.

## The parity rule

The contract is unchanged: `docs/tauri/parity/02-ui-main-window.md` and `03-dialogs.md` (every item is
`[x]`/`[~]` for the Tauri window) are walked again for the egui window in phase E5. A feature the Tauri
window has and egui cannot have becomes a decision below, never a silent loss.

## Architecture

```
crates/lider-core      engine, manager, view model            unchanged
crates/lider-sys       OS ports (Linux, Windows adapters)     unchanged
crates/lider-gui       NEW: the egui program (bin `lider`; `lider-egui` until the cut-over)
  main.rs              process modes (native host, browser CLI, GUI), as src-tauri/main.rs
  app.rs               state (manager, UI state), the 500 ms tick, reactions to manager events
  theme.rs             the three palettes (tokens.ts), egui Visuals and metrics from them
  fonts.rs             the system's UI and monospace fonts plus CJK fallback, found with fontconfig
  icons.rs             the SVG icon set, recoloured per theme (icons.ts)
  widgets/             pill, segment bar, tag input, combo, tab strip, check header …
  main_window/         menu bar, toolbar, tree, list, details, status bar, search bar
  dialogs/             one module per dialog; each dialog is its own OS window (egui viewport)
  tray.rs, updater.rs  tray-icon; download + minisign check + replace (no Tauri plugin)
```

The UI logic that `src-tauri` holds (app.rs, commands.rs, dialogs.rs: 2 900 lines, about 250 Tauri
references) moves into `lider-gui` without the JSON bridge: the window calls the manager directly.

## Phases

| Phase | Content | Check |
|---|---|---|
| E0 | Crate, eframe window, theme, fonts, icons, tick, the real download list | runs on the golden profile; screenshot next to the Tauri one |
| E1 | Main window: menus, toolbar, tree, list (columns, sort, checks, selection, context menus), details pane, status bar with tips, search bar, shortcuts, message boxes | `egui_kittest` port of the 24 main-window checks |
| E2 | Running: single instance, browser requests, clipboard monitor, tray, close/minimise rules, notifications, power-off countdown, drag-and-drop | kittest + manual list |
| E3 | Dialogs (17) as viewports: add flow, batch, links, progress, complete, mismatch, check hash, properties, options, scheduler, browsers, limiter, columns, find, category, font | kittest port of the 28 dialog checks |
| E4 | Updater (minisign, AppImage/Windows), yt-dlp on demand window | unit tests; a signed test release |
| E5 | Parity walk of 02 and 03, side-by-side screenshots in three themes, performance numbers | every item `[x]`/`[~]` |
| E6 | Packaging without WebKit (AppImage, deb, Arch, Windows installer), release workflow, cut-over: remove `src-tauri/`, `ui/` | packages built and run |

## Status

**E0 (2026-11-12): done.** crates/lider-gui, theme, fonts, icons, the real list.

**E1 (2026-11-18): main window built; a few details left.**
- The menu description moved from src-tauri to `lider_core::menu` (one description for both windows).
- `app.rs` holds the actions of the menus, the toolbar and the context menus (the logic of the Tauri
  `commands.rs` without the JSON bridge); `boxes.rs` the message boxes (question, note, input, check box).
- `main_window.rs`: menu bar with checkable items and status tips; toolbar (labels on/off, the Delete
  arrow menu, the checked-rows count); search bar; category tree with its context menus; the list with
  the check column and the header box, sorting by header, click, Ctrl and Shift selection, double-click,
  keys (Up, Down, Home, End, Enter, Space, Delete, Ctrl+A), the row context menu; the details pane (three
  tabs, segment bar, per-download speed limit); status bar; shortcuts Ctrl+N, Ctrl+Q, Ctrl+F, F3, F9.
- Symbols (sort arrows, check marks, close buttons) are drawn or come from the icon set, never from a font
  that might lack the glyph; the tests caught it.
- Tests (`src/tests.rs`, egui_kittest, golden profile): 8 tests with the checks of the Tauri UI scenario:
  layout, tree filters, header sorting, checks and the header box, details pane and F9, search with Match
  case, Esc and F3, Delete with its question, View menu, themes, toolbar captions, status tips.
- Left for E1: dragging column widths and pane sizes (saved), Alt mnemonics in the menu bar, the status
  bar's Browser link, density and list font from the settings, the window size and position saved.

**E2 (2026-11-18): running, done; a few checks by hand left.**
- Start-up in the order of 1.x `app.py`: legacy folders, settings, desktop entries refreshed, the list and
  the manager, the instance socket, the window (hidden when starting in the tray), the command-line URLs.
  `--help`, `--minimized` and a second start (URLs or "show") work as in the Tauri build; the command-line
  parsing moved to `lider_core::cli`, the theme choice to `lider_core::view::resolve_theme`.
- Lines from other starts and the browser helper (download, links, show) and the clipboard monitor reach
  the window; the windows they open (Download File Info, Download All Links, Checksum mismatch) come with
  E3 and show a note until then.
- The manager's events become notifications (complete, failed, checksum mismatch, queue finished), a
  queue's "open when done", "exit when done" and "turn off when done" (the 30 s countdown with Cancel).
- Closing: to the tray when it is up and "close to tray" is on; otherwise, with downloads running, the
  question with Stop and close / Continue in background / Keep downloading (Exit drops the background
  choice when the tray is already there). Exit always ends LID. The window's place and size are saved.
- Tray icon (decision G6): Open LID, Add new download, Speed Limiter on/off, Exit; its tip shows the speed.
- Ctrl+V on the window adds the addresses in the clipboard (decision G2): one opens the add flow, several
  go to the Main queue.
- "Follow the system" theme follows the desktop's light or dark setting while LID runs.
- Engine fix found by the tests: Stop (and closing LID) now also ends a connection whose server has not
  answered yet, instead of waiting for the read timeout (`stop_while_the_server_is_silent`).
- Tests: 10 more kittest/app tests (18 in lider-gui).
- To check by hand (no tray or notification server under Xvfb): the tray on KDE and GNOME, a notification,
  closing to the tray and opening again, starting with `--minimized`.

**E3 (2026-11-19): dialogs, done; a few details left.**
- Every dialog is an egui viewport drawn in the same pass as the main window (`crates/lider-gui/src/dialogs/`);
  under the test harness it is an egui window over the main one. A window is named by its label, so opening
  one that is open brings it forward; windows take the height of what they show. A dialog opened while the
  main window is hidden in the tray asks for one paint of the hidden window, which makes its own window.
- Built: Enter new address, the add flow (probe, "Add it anyway?", the duplicate rule, Download File Info,
  YouTube variants), Batch download, Download All Links and the grabber, the progress window (with its options
  on completion), Download complete, Checksum mismatch, Check hash, File Properties, LID options (7 tabs,
  categories, server limits), Scheduler, Browser integration (and the status bar's Browser link), Speed
  Limiter settings, Columns, Choose Font, Find file / Search settings, the category editor.
- The rules of `src-tauri/src/dialogs.rs` are ported into the dialogs (src-tauri goes away in E6).
- Tests: 27 more kittest checks (45 in lider-gui), including the 28 checks of the Tauri dialog scenario
  except Alt+C on the Scheduler. Every field has an accessible name (kittest checks it).
- `[~]` left: Alt mnemonics (with E1's); the links
  list has no Shift/Ctrl selection; the category form replaces the window's content instead of floating over
  it; Choose Font previews the size, not the family (egui draws only the fonts it loaded).
- To check by hand: a dialog opened from the tray (the browser's "Download with LID" while LID is hidden).

**E4 (2026-11-19): updater and yt-dlp, done.**
- The updater moved out of Tauri into `lider_core::update`: the releases list and the channel (Stable or
  Pre-release), `updates.json` (check at start, channel, skipped version), `latest.json` (the Tauri updater's
  format, so releases of both builds update each other), the minisign check against LID's key (the same key
  as `tauri.conf.json`; a file signed by the real release key verifies), the AppImage replaced whole (written
  beside it, then moved over it), the Windows installer started with `/P /UPDATE /R`. src-tauri reuses it.
- The window: a quiet check 5 s after start (not in a source checkout unless `LIDER_UPDATE_CHECK=1`; it waits
  while another question is open), the offer (Skip this version / Later / Update now, or Open release page for
  a deb or Arch install), "Updating LID" with its progress and Cancel, "LID was not updated" with the reason,
  then LID closes and the new program starts once the old one has let go of its socket. Options → General has
  the Updates panel (check at start, versions, Check now).
- "Download yt-dlp?" and "Downloading yt-dlp" (checked against the release's SHA2-256SUMS); the address that
  needed it is added again afterwards.
- Tests: core (newest version, prefs, AppImage replacement, a local signed release: good signature, another
  key, a changed file, the releases list) and kittest (Check now and Skip, the offer, a refused signature,
  "Download yt-dlp?"). End to end under Xvfb: a release served locally and signed with the real key replaced a
  stand-in AppImage and started the new program. `LIDER_UPDATE_API` and `LIDER_UPDATE_KEY` point a build at a
  test release.
- For E6: the release workflow keeps signing with `cargo tauri signer sign` (or `minisign`) and writing
  `latest.json`; no Tauri is needed to read them.

**E5 (2026-11-19): parity walk, done.**
- `docs/egui/parity/02-ui-main-window.md` (182 `[x]`, 11 `[~]`, 0 open) and `03-dialogs.md` (242 `[x]`, 2 `[~]`,
  0 open): the Tauri checklists walked again for the egui window, each item with an **egui:** note. The walk
  filled what was missing: column widths, sidebar and details pane dragged to size and kept; Alt mnemonics in
  the menu bar, on dialog buttons and check boxes; density and the list font; the toolbar's button kinds; the
  details pane's layout; the links list's selection; the category and server-limit forms floating over their
  window; the Scheduler's calendar; Choose Font previewing the family; folding tree branches; the bold face of
  the UI font for titles, pills, buttons and column headers.
- The `[~]` items are the decisions G2 (links dropped on the window), G3 (native menu bar), the theme engine
  (egui Visuals from the same tokens), no desktop icon theme, window geometry as `window_state`, and two
  dialog details (a menu cannot leave its window; `choose_file` is unused in 1.x).
- Side by side (`tests/egui/side_by_side.sh OUT`): the main window of both builds in the three themes.
- Performance (`tests/egui/perf.sh`, release builds, golden profile, Xvfb with software OpenGL, mean of 3):

  | | Tauri | egui |
  |---|---|---|
  | Window shown after start | 507 ms (page still loading) | 513 ms (drawn) |
  | Memory (PSS of the program and its helper processes) | 383 MiB | 155 MiB |
  | Idle CPU (one 500 ms tick each) | 1.6 % | 3.2 % (software rendering redraws the window) |
  | Program size | 21.4 MiB (+ WebKitGTK) | 25.8 MiB |
  | Shared libraries linked | 143 | 4 |

  A dialog is drawn in the same frame it is asked for (the Tauri build keeps a spare WebView for that).
- 62 tests in lider-gui. To check by hand (no Xvfb can): the tray on KDE and GNOME, a dialog opened from the
  tray, typing CJK with fcitx5/IBus (G4: egui and winit pass IME events), the end of the power-off countdown.

**E6 (2026-11-19): packaging and cut-over, done; the Windows installer needs a try on a real machine.**
- Cut-over: `src-tauri/`, `ui/` and the Tauri UI scenarios (`tests/e2e`) are gone; the egui program is
  `lider` (crates/lider-gui), its icons are in `crates/lider-gui/icons` (the .exe gets its icon and version
  from `build.rs`), the native-host test moved with it. The workspace has no Tauri, WebKit, GTK, Node.js
  or npm left; `lider-pack licenses` lists the Rust crates only.
- `lider-pack sign FILE` writes the updater's signature with the release key (`LIDER_SIGNING_KEY`, or the
  older `TAURI_SIGNING_PRIVATE_KEY`); a file it signed with the real key passes LID's check.
- Linux (`packaging/build.sh`): the AppImage is the program, its desktop entry, icon and licences
  (12 MB, was about 156 MB with WebKitGTK) and runs; the deb (8 MB) depends on EGL/GL, X11, Wayland and
  xkbcommon; the Arch package (10 MB) builds from the source tarball with cargo only.
- Windows: `packaging/windows/lider.nsi` (NSIS), per-user to `%LOCALAPPDATA%\LID` like the Tauri betas
  (same folder, program and uninstall key, so it installs over them), takes the updater's `/P /UPDATE /R`;
  `./packaging/build.sh windows` builds and signs it. Built in CI on windows-latest.
- CI (`rust.yml`): fmt, clippy, tests, golden checks, parity counts of `docs/egui/parity`; Windows: tests,
  clippy, the installer as an artifact. `release.yml` builds AppImage, deb, Arch/AUR, the extension and
  the Windows installer, signs AppImage and installer, and writes `latest.json`.
- `tests/egui/perf.sh` and `side_by_side.sh` compare with another build through `COMPARE_BIN`.
- Left for the user: the secrets `LIDER_SIGNING_KEY` / `LIDER_SIGNING_KEY_PASSWORD` (`packaging/README.md`),
  a try of the installer and of an update from the Tauri beta on Windows, the hand checks of E2 and E5.

## Decisions

| # | Item | Decision |
|---|---|---|
| G1 | Fonts: egui draws text itself and bundles no CJK | Load the system's UI font, monospace font and a CJK fallback at start (fontconfig on Linux, the Fonts folder on Windows); the bundled egui fonts are the last fallback. |
| G2 | Dropping a link from a browser: winit delivers dropped files only | Proposed: paste (Ctrl+V on the window) and the clipboard monitor cover links; a native text drop per OS is looked at in E2 and becomes `[~]` if it cannot be done. |
| G3 | Menu bar mode "Native" (GTK menu bar, D1) | Proposed: the menu bar is always drawn in the window; the setting keeps its three values and treats Native as In window. |
| G4 | Input methods (CJK typing) on Linux are weaker in egui | Accepted risk; checked in E5. |
| G5 | Dialog windows | Each dialog is an egui viewport (a real OS window, like 1.x and the Tauri build), not an overlay. |
| G6 | Tray icon without GTK | Linux: a StatusNotifierItem over D-Bus (`ksni`), the same protocol the Tauri build's tray library speaks, so no GTK is needed; Windows: the shell icon (`tray-icon`). A desktop without a tray gets no icon, and closing then never hides LID. |
