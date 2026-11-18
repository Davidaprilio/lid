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
crates/lider-gui       NEW: the egui program (bin `lider-egui` until the cut-over, then `lider`)
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
- Windows that come with E3 show a note for now ("comes with the dialogs").

## Decisions

| # | Item | Decision |
|---|---|---|
| G1 | Fonts: egui draws text itself and bundles no CJK | Load the system's UI font, monospace font and a CJK fallback at start (fontconfig on Linux, the Fonts folder on Windows); the bundled egui fonts are the last fallback. |
| G2 | Dropping a link from a browser: winit delivers dropped files only | Proposed: paste (Ctrl+V on the window) and the clipboard monitor cover links; a native text drop per OS is looked at in E2 and becomes `[~]` if it cannot be done. |
| G3 | Menu bar mode "Native" (GTK menu bar, D1) | Proposed: the menu bar is always drawn in the window; the setting keeps its three values and treats Native as In window. |
| G4 | Input methods (CJK typing) on Linux are weaker in egui | Accepted risk; checked in E5. |
| G5 | Dialog windows | Each dialog is an egui viewport (a real OS window, like 1.x and the Tauri build), not an overlay. |
