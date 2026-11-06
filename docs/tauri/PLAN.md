# LID 2.0: port to Rust + Tauri

Branch: `tauri` (from `redesign`, so it includes the AppImage menu entry and autostart work in `5b7879c`).
Target release: **LID 2.0.0**. LID stays Linux-only.

## 1. The parity rule

Nothing ships until every feature of LID 1.2 is in the port. The contract is the four checklists in
[`parity/`](parity/), written by reading every line of the Python code and its tests:

| File | Covers | Items |
|---|---|---|
| [`parity/01-core.md`](parity/01-core.md) | engine, manager, queues, scheduler, models, store, settings, categories, checksum, fmt, search, YouTube | 193 |
| [`parity/02-ui-main-window.md`](parity/02-ui-main-window.md) | menus, shortcuts, toolbar, sidebar, list, details pane, status bar, tray, search, tick loop, themes, widgets | 193 |
| [`parity/03-dialogs.md`](parity/03-dialogs.md) | every dialog, tab, field, default and validation rule | 244 |
| [`parity/04-integration-packaging.md`](parity/04-integration-packaging.md) | CLI, IPC, native host, browser setup, desktop entries, extension, packaging, release | 139 |

How it is used:

- A `- [ ]` item becomes `- [x]` only when the Rust/TS code does it **and** a test or a written manual
  check covers it. Add the test name or `manual:` note after the item.
- An item that changes on purpose (a quirk fixed, a platform gap) becomes `- [~]` with a link to an entry in
  §9 (Decisions). Nothing is dropped silently.
- `docs/tauri/parity-progress.sh` prints the open count per file. Cut-over (phase 8) needs zero `- [ ]`.
- The `file:line` references point at the Python code on this branch. It stays in the tree until cut-over, so
  the references keep working.

## 2. What stays the same (compatibility promises)

Users upgrade from 1.x by installing 2.0 over it. Their data, browser setup and running downloads carry over:

- **Data files, same paths and formats:**
  - `~/.config/lider/settings.json`: same keys, same defaults, same migration shims, unknown keys ignored.
  - `~/.local/share/lider/downloads.db`: same SQLite schema, two tables with a JSON blob per row.
  - `~/.cache/lider/parts/<id>.part` plus the saved segment list, so a half-finished 1.x download resumes in 2.0.
  - Legacy folder migration (`lidman`, `lindm`) is kept.
- **Unknown JSON fields are preserved** in Download/queue blobs (serde `flatten` into an `extra` map), so a
  user can go back to 1.2 without losing data.
- **IPC:** same abstract socket `\0lider-<uid>`, same `LIDER_SOCKET_NAME` override, same one-line-per-item
  protocol (URL, `--show`, `{…}` JSON).
- **Native messaging:** same host name `io.github.davidaprilio.lid`, same allowed extension ids, same message
  types and replies. The browser extension does not change, apart from moving out of the Python package.
- **CLI:** `lider [URL …] [--minimized]`, `--native-host`, `--install-browser-integration`,
  `--uninstall-browser-integration`, with the same exit codes and messages.
- **Desktop entries:** same file names, `X-LID-Written-By=LID` marker and `Exec` quoting. `refresh()` at every
  start re-points them at the running AppImage.
- **UI:** the same menus, toolbar, columns, dialogs, labels, shortcuts and the three themes (Ember, Ember Dark,
  Dark) as designed in Figma. Labels stay identical so the docs and screenshots stay valid.

Two things cannot be carried over and are reset once:
- `window_geometry` is a Qt-encoded blob. The port reads its own `window_state` key and falls back to the
  default 1000×560 the first time.
- `list_font` is a Qt font string (`Family,size,…`). The port parses family and point size from it and ignores
  the rest.

## 3. Target architecture

```
Cargo.toml                    workspace; [workspace.package] version is the only version source
crates/
  lider-core/                 no Tauri, no GTK (the rule "core never imports Qt" carries over)
    paths.rs migrate.rs       XDG dirs, legacy folder migration
    models.rs store.rs        Download, Queue, Status; SQLite (rusqlite, bundled)
    settings.rs categories.rs Settings with serde defaults + shims; categories and classify()
    fmt.rs search.rs          size/speed/time text; SearchBar matching and Find Next
    checksum.rs               MD5/SHA-1/SHA-256/SHA-512, SHA256SUMS lines, Verifier
    http.rs ftp.rs            HttpClient (reqwest) and FTP (single connection, as today)
    limiter.rs meter.rs       token bucket shared by global and per-file limits; speed meter
    engine.rs                 DownloadTask: segments, open-ended ranges, splitting, pwrite, resume
    media.rs media_task.rs    yt-dlp JSON, variants, stream download, ffmpeg join/convert
    manager.rs queue.rs       DownloadManager, queues, scheduler, poll() -> Vec<Event>, snapshot()
  lider-sys/                  Linux integration, no GUI
    ipc.rs                    abstract Unix socket client and server
    native_host.rs            native messaging framing, validation, launch of LID
    browser.rs                browser detection, host manifests and wrapper, install/uninstall
    desktop.rs                menu entry, autostart, AppImage refresh
src-tauri/                    the `lider` binary
  main.rs                     dispatch before any GUI init: --native-host / --*-browser-integration / IPC forward / GUI
  state.rs commands.rs        Tauri state and #[tauri::command]s over the manager
  pump.rs                     500 ms tick: manager.poll() -> events + snapshot to the windows
  windows.rs                  main window, one window per dialog, per-download progress windows
  tray.rs menu.rs             tray icon and menu; native menu bar mode
  clipboard.rs power.rs       clipboard watcher; systemctl poweroff after the 30 s countdown
  fonts.rs theme.rs           system font list; system light/dark signal
ui/                           frontend: Svelte 5 + TypeScript + Vite, no CSS framework
  theme/                      tokens of the three themes as CSS custom properties (copied from theme.py)
  icons/                      the Signal SVG set, recoloured with currentColor
  components/                 Shell, Panel, Note, Primary, TabStrip, TagInput, ChecksumEdit, CheckHeader,
                              StatusPill, SegmentBar, VirtualList, MenuBar, Toolbar, Tree
  windows/                    main, add, links (batch/grabber), progress, complete, properties, options,
                              scheduler, find, limiter, columns, server, category, browsers, checkhash,
                              mismatch, closing, add-to-menu, about, duplicate, shutdown
browser_extension/            moved from lider/browser_extension/, unchanged
tests/js/                     extension tests, unchanged
packaging/                    build.sh rewritten around cargo / tauri build
```

### How the pieces talk

- **Manager actor.** `DownloadManager` lives in one tokio task and owns all state, like today. Commands reach
  it over a channel. Engine connections are tokio tasks; they report through shared atomics and a segment
  table, never by calling into the UI.
- **The tick stays.** `pump.rs` runs every 500 ms, calls `manager.poll(now)`, and emits two Tauri events:
  `lid://events` (the `Event` list: `complete`, `error`, `mismatch`, `checked`, `queue_started`,
  `queue_done`, `changed`) and `lid://snapshot` (only the rows that changed). The frontend reacts exactly as
  `MainWindow.handle_events` does. Keeping `poll(now)` with an injected clock keeps the scheduler testable.
- **Windows.** Every dialog that is a top-level window in 1.x is a Tauri window in 2.0, because some must
  appear while the main window sits in the tray (browser capture opens Download File Info). One frontend
  bundle, one route per window (`index.html#/add?…`). Modal dialogs set `parent` and block input to the owner.
- **Native host start-up.** `main.rs` checks argv before Tauri is touched. If loading WebKitGTK's shared
  libraries pushes host start-up over 50 ms, split the host into a second, GUI-free binary `lider-host` in
  the same package (decide in phase 4 from a measurement).

### Crates

| Concern | Choice |
|---|---|
| async runtime | tokio |
| HTTP | reqwest (rustls, `socks` feature), redirects followed, `Accept-Encoding: identity` |
| FTP | suppaftp |
| positioned writes | `std::os::unix::fs::FileExt::write_at` inside `spawn_blocking` |
| SQLite | rusqlite (`bundled`) |
| JSON | serde, serde_json |
| hashes | md-5, sha1, sha2 |
| globs (`server_limits`, wildcards) | globset / own `fnmatch` matching Python's rules |
| Content-Disposition, percent-decoding | percent-encoding + own parser (port of `parse_content_disposition`) |
| HTML links (grabber, "all links") | scraper |
| clipboard | arboard (X11 and Wayland data-control), polled |
| system dark mode | xdg-desktop-portal `color-scheme` via ashpd, Tauri theme event as fallback |
| D-Bus (global menu detection, notifications) | zbus |
| trash ("Delete file" on mismatch) | trash |
| file dialogs, opener, notifications | tauri-plugin-dialog, tauri-plugin-opener, tauri-plugin-notification |
| frontend | Svelte 5, TypeScript, Vite, pnpm |

## 4. Python to Rust map

| 1.x | 2.0 |
|---|---|
| `lider/__init__.py` (version, names) | `Cargo.toml` workspace version; `lider-core::APP_NAME` etc. |
| `lider/app.py`, `__main__.py` | `src-tauri/main.rs` |
| `lider/ipc.py` | `lider-sys/ipc.rs` |
| `lider/native_host.py` | `lider-sys/native_host.rs` |
| `lider/browser.py` | `lider-sys/browser.rs` |
| `lider/desktop.py` | `lider-sys/desktop.rs` |
| `lider/core/*.py` | `lider-core/*.rs`, one module each (engine splits into http/limiter/meter/engine) |
| `ui/main_window.py` | `src-tauri` commands/pump/tray/menu + `ui/windows/main` |
| `ui/download_model.py` | `ui/windows/main/list` (columns, sort keys, check state) |
| `ui/details.py` | `ui/windows/main/details` |
| `ui/widgets.py` | `ui/components/*` |
| `ui/theme.py` | `ui/theme/*.css` + `src-tauri/theme.rs` |
| `ui/icons.py` + `assets/icons` | `ui/icons/` |
| `ui/util.run_async` | async Tauri commands |
| `ui/dialogs/*.py` | `ui/windows/*` |
| `tests/*.py` | Rust tests in each crate, frontend tests, WebDriver UI tests (see §7) |

## 5. Platform gaps to solve (each one is a parity item, not an excuse)

| 1.x feature | Problem in Tauri on Linux | Plan |
|---|---|---|
| Tray icon: left click shows/hides the window | libappindicator ignores clicks; only the menu opens | Keep "Show LID" as the first tray menu item; try StatusNotifierItem `Activate` through ksni if appindicator is not enough. Decide after testing KDE and GNOME. |
| Menu bar modes native/custom/both, global menu via `com.canonical.AppMenu.Registrar` | Tauri cannot export a D-Bus global menu | `custom` (HTML menu bar) always works; `native` uses Tauri's GTK menu bar inside the window. Global menu export needs a decision (§9, D1). |
| Clipboard monitoring (`dataChanged` signal) | no change signal | Poll every 500 ms in `clipboard.rs` and compare with the last text. |
| Drag and drop of links from the browser | Tauri's drop handler only gives file paths | Turn off `dragDropEnabled` and read `text/uri-list` / `text/plain` with HTML5 drag-and-drop. |
| Font picker | the webview cannot list system fonts | `fonts.rs` lists families through fontconfig (`fc-list` or the fontconfig crate). |
| Follow system light/dark | WebKitGTK `prefers-color-scheme` follows GTK, not always the portal | Read the portal `color-scheme` setting and listen for changes; push the theme to all windows. |
| yt-dlp used as a Python module | Rust cannot import it | Run the `yt-dlp` executable with `--dump-single-json --no-playlist` and the same options; the JS runtime goes in `--js-runtimes`. `pip install yt-dlp` installs that executable, so the README instructions barely change. Detection looks for the executable. |
| QSS look (Fusion + palette) | none | Rebuild from the theme tokens and the Figma frames; compare screenshots against 1.2. |
| Per-connection segment bar, status pills, tree badges (Qt delegates) | none | Svelte components drawn with CSS/SVG. |
| Single-instance check | Tauri's single-instance plugin uses D-Bus | Keep LID's own abstract socket, so 1.x and 2.0 extension hosts keep working. |
| RAM and start-up | WebKitGTK uses more memory than Qt Widgets (measured estimate 120–250 MB vs 90 MB) | Budget: start under 1 s, idle under 200 MB with 1,000 downloads in the list. Virtualise the list. Create dialog windows on demand. |

## 6. Phases

Each phase ends with its parity items ticked and its tests green in CI. Python stays runnable throughout.

### Phase 0: foundation
- Install the system packages (Arch): `webkit2gtk-4.1 libappindicator-gtk3 librsvg` and `cargo install tauri-cli`.
- Cargo workspace, `src-tauri` skeleton, `ui/` with Svelte + Vite, `pnpm` scripts.
- CI job `rust` (fmt, clippy, test) next to the Python job; both run until cut-over.
- Version source: `[workspace.package] version`; the release workflow and `build.sh` read it from there.
- `docs/tauri/parity-progress.sh`.
- **Golden data:** a script that uses the 1.2 Python code to write sample `settings.json`, `downloads.db`
  (every status, queues, media, checksum fields) and part files into `tests/golden/`. The Rust tests load them.

**Status 2026-11-06: done.**
- Workspace `Cargo.toml` (version `2.0.0-alpha.1`), `crates/lider-core`, `crates/lider-sys`, `src-tauri`
  (binary `lider`, argv dispatch before the GUI, `app_info` command), `ui/` (Svelte 5 + TS + Vite, port 1420).
- `.github/workflows/rust.yml`: frontend check/build, fmt, clippy `-D warnings`, tests, golden check, parity summary.
  The release workflow keeps reading `lider/__init__.py` until phase 7, because 1.x releases still come from it.
- `tests/golden/make_golden.py` writes `tests/golden/v1.2/` (10 downloads covering every status, a YouTube
  record, checksum fields, four queues, current and legacy settings, part files) plus `expected.json`, which is
  what 1.2 reads back. CI re-runs it and fails if the output changes (the `.db` bytes are not compared).
- Version pin: the `tauri` crate is on 2.11 while npm already has 2.12, and `tauri dev` refuses mismatched
  minor versions, so `ui/package.json` pins `@tauri-apps/api` to `~2.11.1`. Raise both together.
- `cargo tauri dev` opens the window; the native-host and browser-integration modes exit with
  "not ported yet" until phase 4.

### Phase 1: data layer (`lider-core`, no network)
- paths, migrate, models, store, settings (every key and shim), categories, fmt, search, checksum.
- Done when: the golden files load and save back unchanged; `test_manager`/`test_search` cases that need no
  network have Rust equivalents.

**Status 2026-11-06: done.** `crates/lider-core/src/`: `paths` (XDG folders as a `Dirs` value, legacy migration),
`pyutil` (exact ports of `fnmatch`, `splitext`, `urlsplit().hostname`, truthiness), `models`, `queue`
(`QueueSchedule`, `next_start`), `settings` (all 49 keys, shims, plus `window_state`), `categories`, `store`
(rusqlite), `fmt`, `search` (full case folding), `checksum` (parse with the exact texts, `Verifier`).
- 47 unit tests, including a port of every data-layer case in `test_manager.py` and all of `test_search.py`.
- `tests/golden.rs`: the 1.2 profile reads back exactly as 1.2 reads it, and what 2.x writes reads back the same.
- Downgrade check: `tests/golden/check_rust_profile.py` reads the profile written by 2.x with the 1.2 Python code
  and compares it with `expected.json` (also in CI).
- Parity: 42 items done and 4 changed in `parity/01-core.md` (decisions Q14–Q16 accepted 2026-11-06).
  Still open from this area: the startup call of the legacy migration (phase 4) and `remember_last` in the UI (phase 6).

### Phase 2: engine
- http (probe, ranges, Content-Disposition, redirects, proxy, cookies/referrer/UA/auth headers), ftp,
  limiter, meter, engine (segmentation, splitting at 2×MIN_SPLIT, retries, FATAL_HTTP, fallbacks, pwrite,
  resume, move to destination, name handling).
- Port `tests/httpfixture.py` to a Rust test server (ranges, ignore-range, no length, drops, refusals,
  throttling) and port every `test_engine.py` case.
- Benchmark against 1.2 on the local fixture: at least the same throughput at 8 and 32 connections.

**Status 2026-11-06: done.** `crates/lider-core/src/`: `names` (Content-Disposition, URL names, safe names,
unique paths), `limiter` (token bucket, speed meter), `http` (reqwest, HTTP/1.1 only, own redirect loop that keeps
every header, timeout per read, proxy), `ftp` (suppaftp), `engine` (`DownloadTask` on tokio: dynamic segmentation,
retries, fallbacks, resume, pwrite, finish).
- `tests/engine.rs` (19 tests) with `tests/fixture/` (port of `httpfixture.py` plus redirects, a hanging path,
  proxy mode, a range switch and a small FTP server): every `test_engine.py` case plus redirects, 403/404, the
  Range fallback, name collisions, the per-file limit, FTP, proxy and timeouts. Ran 5 times in a row without a failure.
- Benchmark (release build, `examples/bench_server.rs` + `examples/bench_download.rs`, 1.2 through the same server,
  written to tmpfs):

  | Server | Connections | 2.x | 1.2 |
  |---|---|---|---|
  | loopback, 1 GiB, no cap | 1 / 8 / 32 | 4453 / 3868 / 3855 MB/s | 3340 / 3799 / 2741 MB/s |
  | 200 MiB, 20 MB/s per connection | 1 / 8 / 16 | 18 / 146 / 294 MB/s | 18 / 146 / 293 MB/s |

- Parity `01-core.md`: 98 done, 9 changed (decision Q17 accepted 2026-11-06). Open in the engine section, all
  waiting for the manager in phase 3: the single global limiter, the client built from Settings, the periodic save.
- `parity-progress.sh` now also counts numbered items (`1. [ ]`), which adds 15 to the total.
- Fixed on the way: serde_json's default float parser is not exact, so a `date_added` like 1793950472.7764683
  came back one bit off after a save. The workspace enables `float_roundtrip`; test `float_times_round_trip_exactly`.

### Phase 3: manager, queues, scheduler, YouTube
- manager (every public method and event), queues, scheduler with injected clock, checksum flow
  (auto-verify, mismatch policies, check_when_finished, record_check, skip), media + media_task.
- Port `test_manager.py` and `test_media.py` (media tests skip without ffmpeg; the live test stays behind
  `LIDER_NETWORK_TESTS=1`).

**Status 2026-11-06: done.** `crates/lider-core/src/`: `manager` (list, tasks, queues, scheduler, checksum flow,
events, periodic save), `media` (link detection, tool lookup, variants, yt-dlp, ffmpeg commands), `media_task`
(streams through the engine, fresh URLs on every start and after a 403, merge/convert).
- A running task owns its copy of the download; `poll()` copies it into the list, and `edit()`/`update()` send
  the user's fields (description, queue, checksum, address…) to a running task too. Finish reports carry a
  serial, so a stale report from an older task of the same download is ignored.
- yt-dlp runs as a program (`--dump-single-json`), found on PATH, in `~/.local/bin`, or as `python3 -m yt_dlp`;
  the JS runtime goes in `--js-runtimes`. The live probe against YouTube passes (`LIDER_NETWORK_TESTS=1`).
- `tests/manager.rs` (22): every manager case of `test_manager.py` plus N-at-a-time queues, retry limits, the
  scheduler's stop time and once date, start on startup, periodic queues, edits while running, queue
  persistence, stop all, a bad proxy, the global limiter and a YouTube download through the manager.
- `tests/media.rs` (17): every `test_media.py` case plus ffmpeg commands, resolving, no-ffmpeg merging, and
  `labels_match_1_2`, which compares every variant label with the 1.2 output for the saved format list.
- CI installs ffmpeg so the media tests run there. 120 Rust tests in total, 3 full runs without a failure.
- Parity `01-core.md`: 190 done, 12 changed, 6 open; the open ones are UI or startup work (phases 4–6).

### Phase 4: process modes and Linux integration (`lider-sys`, `main.rs`)
- CLI dispatch, IPC client/server (+ peer uid check, §9 Q2), native host, browser install/uninstall,
  desktop entries and refresh.
- Port `test_app.py`, `test_native_host.py`, `test_browser.py`, `test_desktop.py`.
- Cross-check: the 1.2 extension talks to the Rust host, and the Rust host talks to a running 1.2 app.

**Status 2026-11-06: done.** `crates/lider-sys/src/`: `env` (how this copy runs: AppImage, installed, checkout,
binary), `sh` (Python's shell quoting), `ipc` (same socket and protocol; same-user check, a thread per connection),
`native_host`, `browser` (extension built into the program), `desktop`; `src-tauri/src/`: `main.rs` dispatch,
`cli.rs`, and the start-up order of `app.py` in `gui.rs` (migration, settings, desktop refresh, store, manager on
its own tokio runtime, instance socket, hidden start; lines are kept for the window in `take_pending_lines`).
- 46 tests in `lider-sys` (every case of `test_app.py`, `test_native_host.py`, `test_browser.py`, `test_desktop.py`
  except the two UI ones, plus the decisions Q1–Q3, Q5, Q6, Q8) and 2 in `cli.rs`.
- End to end, with temporary folders: a second start hands over the URL and exits in 23 ms, `--minimized` alone
  leaves a running LID alone, the host sees the running app; the 1.2 host talks to the 2.x app and the 2.x host to a
  running 1.2 app.
- Parity `04-integration-packaging.md`: 43 done, 13 changed; what is left there is UI, packaging and release.

### Phase 5: main window
- Theme tokens and the three themes, follow-system, font, density, toolbar labels, icons.
- Menus (all items, shortcuts, mnemonics, enabled rules), menu bar modes, toolbar with the checked-rows chip,
  sidebar tree with badges and saved width, list (columns, sorting, check column with tri-state header,
  pills, column customisation), details pane (F9), status bar, tray, search bar (Ctrl+F) and Find Next (F3),
  drag-and-drop, clipboard monitoring, browser requests, close-while-downloading, power-off countdown.

### Phase 6: dialogs
- Every dialog in `parity/03-dialogs.md`, using the shared `Shell` component: Download File Info (with the
  YouTube box and checksum field), batch download with wildcards, Links/Grabber, batch from clipboard,
  import/export (.ef2 and text), progress (3 tabs, segment bar, connection table), complete, properties,
  Options (7 tabs), Scheduler (2 tabs), find, speed limiter, columns, server login, category, browser
  integration, check hash, mismatch, closing, add to menu, duplicate, about.

### Phase 7: packaging and release
- `build.sh appimage|deb|arch|aur|extension|sdist|all` keeps its interface. AppImage and deb come from
  `tauri build`; Arch builds with cargo in `PKGBUILD`; `sdist` becomes a vendored source tarball
  (`cargo vendor` + `pnpm` offline store) so the AUR package builds offline.
- deb/Arch still register the native host system-wide; postinst/prerm and AppRun are rewritten for the binary.
- Licences: drop the PySide6 notices; ship third-party licences generated with `cargo about` and the npm
  licence list.
- Release workflow: same triggers, version check against Cargo.toml, same artifacts and `SHA256SUMS`,
  Firefox signing unchanged. Update `../aur-lider` (§9 Q10).

### Phase 8: parity audit and cut-over
- Every parity item is `[x]` or `[~]`; side-by-side screenshot review of every window in the three themes.
- Beta AppImage for a week of daily use, including an upgrade from a real 1.2 profile.
- Remove `lider/` (Python), `pyproject.toml` and the Python tests; update README, CLAUDE.md,
  packaging/README.md; release v2.0.0.

Rough size, from the earlier measurement: 10–16 person-weeks. Phases 2, 5 and 6 are the largest.

## 7. Tests

| 1.x test | 2.0 home |
|---|---|
| `test_engine.py` + `httpfixture.py` | `lider-core/tests/engine.rs` + `lider-core/tests/fixture/` |
| `test_manager.py` | `lider-core/tests/manager.rs` |
| `test_media.py` + `data/youtube_formats.json` | `lider-core/tests/media.rs` (same JSON fixture) |
| `test_search.py` | `lider-core/src/search.rs` unit tests |
| `test_app.py` | `src-tauri/tests/cli.rs` |
| `test_native_host.py` | `lider-sys/tests/native_host.rs` (incl. "host mode never starts the GUI") |
| `test_browser.py`, `test_desktop.py` | `lider-sys/tests/` with `XDG_*` in temp dirs, as today |
| `test_ui.py` | Vitest for components + WebDriver tests (`tauri-driver` + WebKitWebDriver) for menus, toolbar captions, tree labels, dialogs |
| `tests/js/capture.test.cjs` | unchanged |

The rule from CLAUDE.md stays: tests never touch the real home.

## 8. Risks

- **Engine correctness** is the core of the app. The golden part files and the ported fixture tests are the
  safety net; do not start the UI before phase 2 is green.
- **WebKitGTK differences** between distros (Arch is current, Debian 12 / Ubuntu 22.04 ship older 4.1
  builds). Test the deb on Ubuntu 22.04 in CI with WebDriver.
- **Tray behaviour** differs between KDE, GNOME (needs the AppIndicator extension) and others.
- **Wayland:** clipboard polling and window positioning (progress windows) behave differently from X11.

## 9. Decisions (owner: David)

Open items found while reading the code. **2026-11-06: every suggestion below was accepted** and is now
the decision. A parity item that changes because of one of them becomes `[~]` with a link to its row.
D2 and D4 still need a measurement or a test in their phase; record the outcome in the row.

| # | Item | Phase | Decision |
|---|---|---|---|
| D1 | Global D-Bus menu (menu bar mode `native`/`both`) cannot be exported by Tauri | 5 | Keep the three modes; `native` = GTK menu bar in the window. Revisit global menu later. |
| D2 | Tray left-click | 5 | See §5; decide after testing. |
| D3 | Frontend framework | 0 | Svelte 5 (small bundle, little runtime). |
| D4 | One binary or a separate `lider-host` | 4 | **Measured 2026-11-06: one binary.** `lider --native-host` (release, linking WebKitGTK) answers `hello` in 23 ms median (1.2: 32 ms; budget 50 ms). |
| Q1 | `lider --minimized` while running shows the window (it sends `--show`) | 4 | Change: do nothing when already running. |
| Q2 | IPC socket has no peer uid check; any local user can add downloads | 4 | Change: check `SO_PEERCRED`. |
| Q3 | IPC read blocks the UI for up to 2 s per slow client | 4 | Fixed by design (async server). |
| Q4 | `mime`, `size`, `final_url` from the browser are ignored by the UI | 5 | Keep ignoring, or use `size` to prefill. |
| Q5 | Host accepts `size: true` as 1; `referer`/`page_url` not URL-checked | 4 | Change: reject non-integers, validate URLs. |
| Q6 | Host error texts are Python-specific (`repr`, JSON decoder messages) | 4 | Change to fixed texts; the extension only checks `ok`. |
| Q7 | deb auto-launch misses `/usr/lib/lider/vendor` | 7 | Gone with Python; verify launch from the host. |
| Q8 | No system host folders for Vivaldi, Opera, LibreWolf; CLI message omits LibreWolf | 4 | Change: add them. **Done in part:** LibreWolf is in the message. The system folders are not added, because no official source confirms them (only a Gentoo packager uses `/etc/opt/vivaldi/native-messaging-hosts`); per-user registration covers these browsers. Revisit with a confirmed path. |
| Q9 | Extension menu actions ignore the on/off switch; `handled` set grows forever | later | Extension change, separate `EXTENSION_VERSION` bump; not part of the port. |
| Q10 | `../aur-lider` is stale at 1.0 and lacks the browser step | 7 | Update with 2.0. |
| Q11 | deb Homepage-removal line can never run | 7 | Remove it. |
| Q12 | Pause All and Stop All do the same; only Stop All has an enabled rule | 5 | Keep both items, give Pause All the same rule. |
| Q13 | "Grabber projects" tree node is always empty | 5 | Keep as is (parity). |
| Q14 | A settings value of the wrong JSON type was stored as is and could fail later | 1 | **Accepted 2026-11-06:** that key keeps its default; the rest of the file loads. |
| Q15 | One unreadable row in `downloads.db` (or `QueueSchedule(**data)` with an unknown key) stopped 1.x from starting | 1 | **Accepted 2026-11-06:** skip the bad row, leave it in the file, report it (the UI can show a note in phase 5); unknown queue keys are kept like download keys. |
| Q16 | Verifier error text is Python's (`[Errno 2] No such file or directory: '<path>'`) | 1 | **Accepted 2026-11-06:** Rust's OS message plus the path. The text only appears after "Checksum check failed:". |
| Q17 | Engine edge cases in 1.x: a failed disk write left zeros in the file; invalid input killed a connection's thread; `socks5://` proxies did not work; an `ftp://` folder URL downloaded the listing | 2 | **Accepted 2026-11-06:** give the bytes back and retry the write; fail the connection with a message; SOCKS works; a folder URL is an error. |

## 10. Prerequisites on this machine

Checked 2026-11-06: rustc 1.98.1, cargo, tauri-cli 2.12.4, node 24, pnpm, bun, and the system libraries
(webkit2gtk-4.1 2.52.6, gtk3 3.24.52, libsoup3 3.6.6, ayatana-appindicator 0.6.0, librsvg) are present.

On a fresh Arch machine:

```sh
sudo pacman -S --needed webkit2gtk-4.1 libappindicator-gtk3 librsvg base-devel
cargo install tauri-cli --locked
```

Gotcha on this machine: Linuxbrew's `pkg-config` comes first in `PATH` and does not search
`/usr/lib/pkgconfig`, so the GTK/WebKit `-sys` crates fail to build. Build with
`PKG_CONFIG=/usr/bin/pkg-config` (export it in the shell, or put it in an uncommitted `~/.cargo/config.toml`
`[env]` section).
