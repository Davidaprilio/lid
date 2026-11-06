# LID UI port checklist (Rust + Tauri)

I read every line of `lider/ui/main_window.py` (1583 lines), `download_model.py` (207), `details.py` (407), `widgets.py` (962), `theme.py` (354), `icons.py` (109), `util.py` (95), `__init__.py` (empty) and `tests/test_ui.py` (453). To fill in context I also looked briefly at `core/settings.py` (DEFAULT_COLUMNS and defaults), `app.py:95-135` (single instance and start minimized), `dialogs/find.py:95-130` (SearchBar), `core/models.py:10-11` (queue names) and `dialogs/add.py:47` (URL_RE). Unless a path says otherwise, references are `file:line` under `lider/ui/`. MW = `main_window.py`.

---

## 0. Global constants and window shell
- [x] Window title is `"Linux Internet Downloader {__version__}"` and the window icon is `app` (MW:98-99). APP_NAME is "LID" and the package is "lider". — **2.x:** `gui.rs` sets the title; the icon comes from the bundle
- [~] The default size is 1000×560 (MW:101). If `settings.window_geometry` is set, the saved geometry (base64) is restored (MW:110-111). — **2.x:** default 1000×560; the geometry is LID's own `window_state` (x, y, size, maximised); the Qt blob of 1.x is kept for 1.x but not read (PLAN §2)
- [x] The window accepts drops (MW:100). — **2.x:** HTML drag-and-drop on the whole window (`Main.svelte`)
- [x] Constants: — **2.x:** `lider_core::models`, `view::is_url`
  - `MAIN_QUEUE = "Main download queue"` and `SYNC_QUEUE = "Synchronization queue"` (core/models.py:10-11).
  - `URL_RE = ^(https?|ftp)://\S+$`, case-insensitive (dialogs/add.py:47).
- [x] Init order (MW:103-128): — **2.x:** `gui.rs`/`Main.svelte`: theme follows the portal (polled every 2 s), limiter on startup, clipboard and the 500 ms tick in `pump.rs`
  1. actions, menus, toolbar, central area, status bar, tray;
  2. restore geometry;
  3. apply the list font from `settings.list_font`;
  4. apply the theme, then subscribe to system colour-scheme changes, which re-apply it;
  5. if `speed_limiter_on_startup`, turn the limiter on;
  6. sync the limiter radio items;
  7. connect the clipboard listener;
  8. start the 500 ms timer and run one `tick()` straight away.
- [x] Search state is restored from `settings.search_string`, and the flags are `settings.search_flags & 0x1F` (MW:92). — **2.x:** `app::new_ui`

## 1. Menus (MW:144-282)
All items are actions shared between the native menu bar and the in-window menu bar. `&` marks the Alt mnemonic. Underlines are hidden, but Alt+letter still works (theme.py:332-338).

### Tasks (`&Tasks`, MW:223-231)
| Label | Shortcut | Icon / tip | Action |
|---|---|---|---|
| `&Add new download` | Ctrl+N | `add-url`, tip "Add new download address manually" | `add_url()` opens AddUrlDialog (§12) |
| `Add &batch download` | – | – | BatchDialog, then `_add_many` with the dialog's credentials (MW:924-928) |
| `Add batch download from &clipboard` | – | – | Splits the clipboard text on whitespace and keeps URL_RE matches. With none, shows info "Cannot find any web address in the clipboard!". Otherwise LinksDialog, then `_add_choices(choices,"","")` (MW:930-938) |
| `Run site &grabber` | – | `grabber`, tip "Run site grabber" | §12 Grabber (MW:940-971) |
| separator | | | |
| submenu `E&xport`: `To .ef2 export &file`, `To &text file` | | | §13 |
| submenu `&Import`: `&From .ef2 export file`, `From &text file` | | | §13 |
| separator | | | |
| `&Exit` | Ctrl+Q | – | `quit_app()` sets `really_quit=True`, then `close()` (MW:1544) |

### File (`&File`, MW:233-234)
| Label | Shortcut | Icon / tip | Enabled when (MW:712-728) | Action |
|---|---|---|---|---|
| `&Stop Download` | – | `stop`, "Stop the checked (or selected) downloads" | any target is running or VERIFYING | `manager.stop(i)` for each target (MW:1043) |
| `&Remove` | Del | `delete`, "Remove the checked (or selected) files from the list" | there is any target | confirmation box (§12) |
| `&Download Now` | – | `resume`, "Start or resume the checked (or selected) downloads" | any target is not running and not COMPLETE/VERIFYING | start each such target. The progress window opens only when exactly one target was chosen and the details pane is off (MW:1034-1041) |
| `R&edownload` | – | – | there are targets and none is running | `manager.redownload(i)` for each, then refresh (MW:1047) |

"Targets" means the checked rows, or the selected rows when nothing is checked (`target_ids`, MW:656-658).

### Downloads (`&Downloads`, MW:236-254)
- [x] `&Pause All` calls `manager.stop_all`. It is always enabled and has no icon (MW:163). — **2.x:** `run_menu`
- [x] `&Stop All` (icon `stop-all`, tip "Stop all downloads") also calls `manager.stop_all`. It is enabled only while `manager.tasks` is non-empty (MW:164, 727). — **2.x:** `menu.rs` enabled rule
- [x] separator — **2.x:** done
- [x] `&Delete All Completed` (icon `delete-completed`, tip "Remove all completed files from the list") is enabled when any download is COMPLETE (MW:165, 728). Action in §12. — **2.x:** done
- [x] separator — **2.x:** done
- [x] `Find (Ctrl-F)`, shortcut Ctrl+F, icon `search`: `show_searchbar` (§10). — **2.x:** e2e `tests/e2e/run.sh`
- [x] `Find Next (F3)`, shortcut F3: `find_next` (§10). — **2.x:** e2e `tests/e2e/run.sh`
- [x] separator — **2.x:** done
- [x] `S&cheduler` (icon `scheduler`, tip "Scheduler and download queues") opens `SchedulerDialog(manager, queue=MAIN_QUEUE, on_events=handle_events)` modally, then rebuilds the tree and refreshes the rows (MW:1107-1110). — **2.x:** the menu item opens the Scheduler window; its window is built (phase 6)
- [x] Submenu `Start &queue` is rebuilt every time it opens (aboutToShow). It lists `_queue_names()`: Main, then Synchronization, then the rest sorted. An item is enabled when that queue is **not** running. Picking one runs `handle_events(manager.start_queue(q))` and refreshes (MW:244-247, 1093-1102). — **2.x:** `menu.rs`: Main, Synchronization, the rest sorted; enabled when not running
- [x] Submenu `S&top queue` uses the same list. An item is enabled when the queue **is** running, and picking it calls `manager.stop_queue(q)`. — **2.x:** done
- [x] separator — **2.x:** done
- [x] Submenu `Speed &Limiter`: — **2.x:** Turn on/off done (shared with the tray); Settings… opens the limiter window, its window is built (phase 6)
  - `Turn o&n` and `Turn of&f` are checkable and exclusive (QActionGroup). Each calls `set_limiter(on)`, which sets the manager's limiter, re-syncs the checks and saves settings (MW:1187-1195).
  - separator
  - `S&ettings...` opens SpeedLimiterDialog. On OK it re-applies `speed_limiter_on` and saves (MW:1197-1200).
- [x] separator — **2.x:** done
- [x] `&Options` (icon `options`, tip "Integration, file types, save folders, connections, proxy and others") calls `open_options("")` (§16). — **2.x:** its window is built (phase 6)

### View (`&View`, MW:256-273)
- [x] `Hide &categories` is checkable, initially checked = `!show_categories`, and calls `toggle_categories(checked)` (MW:180, 1202-1205). — **2.x:** done
- [x] `&Details pane` is checkable with shortcut **F9**, initially = `show_details`, and calls `set_details_visible(on)` (MW:182, 1230). — **2.x:** e2e `tests/e2e/run.sh`
- [x] `Status &bar` is checkable, initially = `show_statusbar`, and calls `set_statusbar_visible` (MW:184, 1235). — **2.x:** done
- [x] separator — **2.x:** done
- [x] Submenu `&Arrange files` holds exclusive checkable items, each calling `arrange(key)`: `sort_key=key`, `sort_desc=False`, then refresh (MW:708). The current item is checked from `settings.sort_key`. Items, in order (MW:68-80): — **2.x:** `menu.rs` `ARRANGE`
  `By &Order Of Addition`(date_added), `By &File Name`(filename), `By &Size`(size), `By St&atus`(status), `By &Time Left`(time_left), `By Transfer &Rate`(rate), `By &Last Try Date`(last_try), `By &Description`(description), `By Save &Path`(save_to), `By R&eferer`(referer), `By Parent &Web Page`(parent_page).
- [x] Submenu `&Toolbar` holds the exclusive items `Icons and &labels` and `&Icons only`, which call `set_toolbar_labels(True/False)` (MW:193-198). — **2.x:** e2e `tests/e2e/run.sh`
- [x] Submenu `&Menu bar` holds the exclusive items `&Native (desktop menu)`(native), `In the &window`(custom) and `&Both`(both), which call `set_menubar_mode` (§3). — **2.x:** done
- [x] `C&ustomize URL List...` calls `customize_columns()`: ColumnsDialog, and on OK `model.set_columns()`, refresh, then re-apply the column widths (MW:1254-1258). — **2.x:** its window is built (phase 6)
- [x] Submenu `T&heme`: — **2.x:** e2e `tests/e2e/run.sh`
  - `Follow the system`
  - separator
  - `Ember`, `Ember Dark`, `Dark`

  These are exclusive and checkable, and each calls `choose_theme(key)` (MW:200-207, 1260-1272).
- [x] Submenu `Font` (no mnemonic): — **2.x:** Reset done; Choose Font… opens its window (phase 6) (system fonts come from fontconfig, `system_fonts`)
  - `Choose Font...`: QFontDialog seeded with the list's font. On OK it sets the list font and stores `font.toString()` in `list_font`; it does not save to disk immediately (MW:1282-1286).
  - `Reset to default Font`: the list returns to the app font and `list_font=""` (MW:1288).
- [x] separator — **2.x:** done
- [x] `A&ppearance...` is **the last item** and calls `open_options("Appearance")` (MW:1167). A test asserts this. — **2.x:** e2e `tests/e2e/run.sh`; the Options window it opens is phase 6

### Help (`&Help`)
- [x] `&About LID` opens an About box titled "About LID" (MW:1292-1298) with this HTML: `<b>Linux Internet Downloader {ver}</b><br>LID · package lider<br><br>A fast download manager for Linux: multi-connection downloads with dynamic segmentation, queues, scheduler, categories, YouTube, clipboard monitoring and search.<br><br>MIT License.<br><br><small>LID is an independent project, not affiliated with Tonec Inc. or Internet Download Manager.</small>` — **2.x:** same text in a message box

Note: per the project rule this is the one allowed mention apart from the README. The port must keep it neutral.

### Menu bar order
- [x] Top level: `&Tasks, &File, &Downloads, &View, &Help`. A test asserts this. — **2.x:** e2e `tests/e2e/run.sh`

## 2. Keyboard shortcuts (complete list)
- [x] Ctrl+N: Add new download. — **2.x:** in-window: `Main.svelte` keys; native: menu accelerators
- [x] Ctrl+Q: Exit. — **2.x:** done
- [x] Del: Remove. — **2.x:** e2e `tests/e2e/run.sh`
- [x] Ctrl+F: show the search bar. — **2.x:** e2e `tests/e2e/run.sh`
- [x] F3: Find Next. — **2.x:** e2e `tests/e2e/run.sh`
- [x] F9: toggle the Details pane. — **2.x:** e2e `tests/e2e/run.sh`
- [x] Esc inside the search bar closes it (dialogs/find.py:126). — **2.x:** e2e `tests/e2e/run.sh`
- [x] Return in the search field runs Find (find.py:108). — **2.x:** e2e `tests/e2e/run.sh`
- [x] Alt+mnemonics work on all menus, with the underline hidden. — **2.x:** in-window menu bar opens on Alt+letter and underlines while Alt is held; GTK handles `&` in native mode

## 3. Menu bar modes (MW:1214-1228)
- [x] An invalid mode becomes "native", and the mode is stored in `settings.menubar`. — **2.x:** `run_menu`
- [~] native: uses the desktop global menu. The window's own menu bar is visible, and Qt falls back to an in-window bar when there is no global menu. The inline toolbar is hidden. — **2.x:** a GTK menu bar inside the window; a global menu cannot be exported (PLAN §9 D1)
- [x] custom: the native menu bar is off and hidden, and the inline menu bar is shown in its own toolbar row (object `menubar`) above the main toolbar (MW:302-308). — **2.x:** done
- [~] both: native is on. The inline bar shows only when a global menu exists, detected as DBus service `com.canonical.AppMenu.Registrar` on the session bus (theme.py:61-71). — **2.x:** the in-window bar shows when the registrar exists (`system::global_menu_available`), next to the GTK bar (PLAN §9 D1)
- [x] The inline menu bar holds the same five menus (MW:279-282). — **2.x:** done

## 4. Toolbar (MW:284-342)
- [x] It is one row, not movable, object `toolbar`. Order: — **2.x:** e2e `tests/e2e/run.sh`
  1. **Add URL**: action a_add, kind `primary`, icon `add-white` (overrides add-url).
  2. separator
  3. **Start**: a_resume, kind `pill`, icon resume.
  4. **Stop**: a_stop, kind `pill`, icon stop.
  5. separator
  6. **Delete**: a_remove, kind `danger`, icon delete. It is a split button (MenuButtonPopup) with the menu "Delete checked or selected" (icon delete → remove_selected) and "Delete all completed" (icon delete-completed → delete_completed) (MW:316-321).
  7. separator
  8. **CheckChip** "N checked ✕". It is visible only while N > 0, where N is the number of checked rows. ✕ unchecks everything (MW:322-323, 716-717).
  9. expanding spacer
  10. **Options** (ghost, a_options), **Scheduler** (ghost, a_scheduler), **Grabber** (ghost, a_grabber).
- [x] Button captions are the short `iconText`; the menus keep the long text (MW:289). A test asserts the captions `["Add URL","Start","Stop","Delete","Options","Scheduler","Grabber"]`. — **2.x:** e2e `tests/e2e/run.sh`
- [x] Buttons mirror their action's enabled state and tooltip, and use the pointing-hand cursor. — **2.x:** disabled state and tooltips from the actions; pointer cursor
- [x] Sizes as (padding, icon px, gap, fixed height): primary (16, 17, 8, 38); ghost (11, 20, 7, 36); pill/danger (13, 17, 7, 34) (MW:286, 297). — **2.x:** CSS heights 38/36/34 and paddings 16/11/13
- [x] Label visibility: with "Icons only" every button except primary (Add URL) becomes icon-only. Add URL always keeps its label (MW:1207-1212). Setting: `toolbar_labels`. — **2.x:** e2e `tests/e2e/run.sh`

## 5. Categories / Queues sidebar (MW:345-371, 432-442, 512-625)
- [x] Panel header: "CATEGORIES" (sectionTitle style) and a close button (icon close, tooltip "Hide categories") that hides the panel (`toggle_categories(True)`). — **2.x:** done
- [x] Panel minimum width is 170. It sits in a horizontal splitter (object `sidebar`) that cannot collapse. Initial sizes are `[settings.sidebar_width (default 236), 800]`. — **2.x:** min 170, the right side at least 560
- [x] The width is persisted on save when the panel is visible (MW:1517-1518). The right side has a minimum width of 560, which is the details pane minimum and applies whether or not the pane shows (MW:430). — **2.x:** `ui_pane_sizes`
- [x] Tree: header hidden, indentation 16, custom TreeDelegate with a count badge. — **2.x:** done
- [x] Nodes (MW:524-535): — **2.x:** `crates/lider-core/tests/view.rs` `category_tree`
  - `All Downloads` (icon folder-download, **expanded**), `Unfinished` (unfinished), `Finished` (finished). Each has one child per category except "General", using the icon from `category_icon(name)`: `category-<lower>` if it exists, else `folder`. The defaults are Compressed, Documents, Music, Programs and Video.
  - `Grabber projects` (grabber). It is a placeholder and always shows an empty list.
  - `Queues` (queue, **expanded**) with children Main download queue, Synchronization queue (icon queue-sync), then the other queues sorted (icon queue).
- [x] Node data is `(kind, value)`: all, unfinished, finished, grabber, queues (value None), queue (value = name). — **2.x:** `view::TreeFilter`
- [x] Counts (badge, refreshed every tick, MW:540-552): — **2.x:** `crates/lider-core/tests/view.rs` `category_tree`
  - all/unfinished/finished: matching downloads, filtered by category when there is a value. Finished = COMPLETE; unfinished = not COMPLETE.
  - queue: downloads in that queue that are not COMPLETE.
  - others: 0.
  - No badge is drawn for 0.
- [x] A rebuild keeps the current node; if it no longer exists, the first node is selected (MW:554-560). The tree is rebuilt after a theme change (so icons recolour), after Options, after the Scheduler and after category edits. — **2.x:** `view::keep_filter`
- [x] Changing the selection sets `tree_filter`, resets Find Next and refreshes the rows (MW:562-566). — **2.x:** `ui_filter`
- [x] Filtering (`visible_downloads`, MW:627-647): — **2.x:** `crates/lider-core/tests/view.rs` `tree_filters`, e2e `tests/e2e/run.sh`
  - unfinished = not COMPLETE; finished = COMPLETE; queue = d.queue == name; **queues node = every download with any queue**; grabber = none.
  - The category value filters all, unfinished and finished.
  - The search filter (§10) is applied next, and an invalid search is ignored here.
  - A queue node sorts by `queue_order`. Everything else uses SORT_KEYS[sort_key] (fallback date_added) with `sort_desc`.
- [x] Context menu (MW:568-584): — **2.x:** queue items and Delete category done; Add category / Properties open the Category window, its window is built (phase 6)
  - On a queue node: `Start` → start_queue(name), `Stop` → stop_queue(name), separator, `Scheduler...` → open_scheduler(name).
  - On the Queues root: only `Scheduler...` (opens MAIN_QUEUE).
  - Anywhere else, including empty space: `Add category` (CategoryDialog, appended if the name is unique, then save and rebuild), `Delete` and `Properties`. Delete and Properties are enabled only on a category child (value is not None).
  - Delete category (MW:612-624): asks the question titled "Deleting a category from LID categories list" with text `Do you really want to delete "{name}" category from LID categories list?`. On Yes it removes the category, moves its downloads to "General" (manager.update), saves, resets the filter to ("all", None) and rebuilds.
  - Edit category (MW:597-610): CategoryDialog(cat). On OK it replaces the category; on a rename it updates every download's category (manager.update), then saves and rebuilds.

## 6. Download list (MW:384-416, 627-780; download_model.py; widgets.py ListDelegate/CheckHeader)
### Columns
- [x] Column 0 is always the checkbox column `_check`: width 40, fixed, empty header with a tri-state box (download_model.py:18, 36; MW:678-682). — **2.x:** e2e `tests/e2e/run.sh`
- [x] The rest are the visible columns from `settings.columns` in order. Defaults, as (key, title, visible, width) (core/settings.py:29-42): — **2.x:** `crates/lider-core/tests/view.rs` `rows_cells_and_columns`
  queue "Q" ✓ 34 · filename "File Name" ✓ 280 · size "Size" ✓ 92 · status "Status" ✓ 180 · time_left "Time left" ✓ 100 · rate "Transfer rate" ✓ 120 · last_try "Last Try Date" ✓ 130 · description "Description" ✓ 200 · date_added "Date Added" ✗ 130 · save_to "Save To" ✗ 200 · referer "Referer" ✗ 200 · parent_page "Parent Web Page" ✗ 200.
- [x] Header labels are shown **UPPERCASE** (download_model.py:105) and left-aligned. Sections are not movable, the last section stretches, and sections are clickable. — **2.x:** done
- [x] Minimum widths: queue 34, size 88, status 170, time_left 96, rate 116 (MW:83). — **2.x:** `view::MIN_WIDTH`
- [x] A resized width is written back into `settings.columns` (MW:685-690). — **2.x:** `ui_column_width`
- [x] Right-clicking the header opens Customize columns (MW:411-412). — **2.x:** right-click opens Customize columns, its window is built (phase 6)
- [x] Cell contents (download_model.py:108-191): — **2.x:** `crates/lider-core/tests/view.rs` `rows_cells_and_columns`
  - filename: text plus a category icon.
  - queue: **no text**. It shows the `queue` icon when the download is queued, with the queue name as tooltip.
  - size: `fmt.size(size)`.
  - status: no text; a pill is drawn (§6 Status pill). The tooltip is `d.error` when the status is ERROR.
  - time_left: empty unless a task is running. Otherwise `fmt.duration((size-downloaded)/rate)` when rate > 0 and size > 0, else empty.
  - rate: `fmt.rate(task.meter.rate())` while running.
  - last_try and date_added: `fmt.date`.
  - description; save_to = full `d.path`; referer; parent_page.
  - size, time_left and rate use the system **monospace** font.
- [x] Status pill data, as `(kind, progress, text)` (download_model.py:138-160): — **2.x:** `crates/lider-core/tests/view.rs` `status_pills`
  - VERIFYING, or "pending verification" (COMPLETE, has checksum, no result yet, still in tasks, verify_checksum on): ("run", p, "Verifying NN%").
  - COMPLETE: verified gives ("verified", 1, "Verified"); mismatch gives ("mismatch", 1, "Checksum mismatch"); otherwise ("done", 1, "Complete").
  - ERROR: ("error", progress, "Error 404") when the error matches `HTTP (\d{3})`, else "Error".
  - Running: ("run", progress, text). The text is "12.3%" while DOWNLOADING with a known size, `fmt.size(downloaded)` with an unknown size, and `fmt.status(d)` in any other running state.
  - downloaded > 0: ("pause", progress, "12.3% · Paused"), or "{size} · Paused" with an unknown size.
  - else ("queue", 0, "Queued").
- [x] Status pill drawing (widgets.py:916-935, `pill()` 517-543): — **2.x:** CSS pills (fill at alpha 70, 3 px edge, ✓ and ! marks, shield icons)
  - Text font: bold, 0.88× the list font.
  - run/pause: fixed width 150 with mono text. The background is `track`, the fill is the progress fraction in the colour at alpha 70/255 plus a solid 3 px left edge in the colour. The colour is `ac` for run and `fa` for pause; the text is `tx`.
  - done: "✓ Complete", `ok` text on `ok`@34 alpha.
  - verified: shield-check mark icon (12 px) and text, `ok` on `ok`@34.
  - mismatch: shield-alert mark, `bad` on `bad`@30.
  - error: "! Error …", `bad` on `bad`@30.
  - queue: `mu` on `track`.
  - Geometry: height = min(row−8, font height+8); width = text+20 (+16 with a mark), capped at cell−8; x inset 4; fully rounded (radius h/2); text left padding 10 (+16 with a mark).
- [x] Row painting (widgets.py:899-914): a checked row that is not selected gets an `acsel`@150 alpha tint; the selected row gets a 3 px `ac` bar at the left of column 0. Rows alternate colours. — **2.x:** done
- [x] Density: row height is 32 for comfortable and 26 for compact (`settings.density`, MW:82, 1276). It is applied when the theme is applied. — **2.x:** 32/26 px
- [x] The view has uniform row heights, no root decoration, all columns showing focus, ExtendedSelection by rows, and dragging rows disabled. — **2.x:** click, Ctrl and Shift selection; no row dragging
- [x] The list font comes from `list_font` (MW:112-115, 1180). — **2.x:** `--list-font`

### Sorting
- [x] Clicking a header for a key in SORT_KEYS (the check column is ignored): the same key toggles `sort_desc`; a new key sorts ascending. It shows the sort indicator and syncs the Arrange menu checks (MW:692-706). — **2.x:** e2e `tests/e2e/run.sh`
- [x] SORT_KEYS (download_model.py:194-207). Port these exact keys, including the odd ones: — **2.x:** `crates/lider-core/tests/view.rs` `sorting_uses_the_1x_keys`
  - date_added → id
  - filename → lowercase name
  - size
  - status → (status.value, progress)
  - **time_left → size−downloaded**
  - **rate → downloaded**
  - last_try
  - description → lowercase
  - save_to → lowercase save_dir
  - referer
  - parent_page
  - queue → (queue, queue_order)
- [x] Arrange-menu sorting does not set the header indicator. — **2.x:** the indicator shows only after a header click

### Selection vs checked
- [x] The checked set is separate from the selection. Ticking a row's checkbox (CheckStateRole on column 0) toggles that id (download_model.py:92-100). — **2.x:** done
- [x] When the visible rows change, checks are pruned to the visible ids (download_model.py:44-47). — **2.x:** `view::prune_checks`
- [x] Header box state (download_model.py:60-71): Unchecked with 0 checked, Checked when all are checked, Partial otherwise. Clicking header section 0 toggles all: if all are checked it unchecks all, otherwise it checks all (widgets.py:793-798). — **2.x:** e2e `tests/e2e/run.sh`, `crates/lider-core/tests/view.rs` `checked_rows_drive_start_stop_delete`
- [x] The header box is 16 px, centred, drawn with `_box` (§17). — **2.x:** done
- [x] Toolbar and File actions act on targets = checked, or the selection when nothing is checked. The **context menu acts on the selection** (selected_ids). Its Start/Stop/Remove/Redownload items are the shared actions, so those still use target_ids. — **2.x:** `view::targets`; the context menu uses the selection
- [x] `refresh_rows` (MW:660-674): rebuilds the rows. If the id order changed, it restores the selection and the current row by id and re-applies the column widths. It always calls `update_actions`. — **2.x:** selection is kept by id
- [x] Changing the selection or current row updates the actions and the details pane (MW:413-415). — **2.x:** done

### Double-click (MW:765-775)
- [x] COMPLETE: Properties dialog. — **2.x:** opens the Properties window, its window is built (phase 6)
- [x] Otherwise: start the download if it is not running, then `show_progress`, which respects `show_progress_dialog`. — **2.x:** starts the download; the progress window is its window is built (phase 6)

### Context menu (MW:730-763)
- [x] With no selection: only `&Add new download`. — **2.x:** done
- [x] Otherwise, with d = the first selected item and "complete" = d is COMPLETE: — **2.x:** Open, Open with…, Open folder, Redownload, Start/Stop/Remove, queues and Copy are done; Move/Rename, Properties and Check hash open their windows (phase 6)
  - `Open`: enabled if complete. Opens the file (`open_path`).
  - `Open with...`: enabled if complete. Input "Open with" / "Command (the file path is appended):". Runs `shlex.split(cmd)+[path]` in a new session (MW:1078-1083).
  - `Open folder`: if complete, `open_folder(path)` (DBus FileManager1.ShowItems selects the file; on OSError it opens the dirname). Otherwise it opens `save_dir`.
  - separator
  - `Move/Rename`: Properties dialog.
  - `R&edownload` (shared action).
  - `Check hash…` (icon checksum): enabled only when **exactly one** row is selected and `can_check(manager, d)`. Opens CheckHashDialog; when it returns a choice, `_mismatch_choice` handles it (MW:1404-1409).
  - separator
  - `&Download Now`, `&Stop Download`, `&Remove` (shared actions).
  - separator
  - Submenu `Add to queue`: one item per queue name, running `set_queue(ids, q)`.
  - `Delete from queue`: `set_queue(ids, "")`, enabled if any selected row is queued.
  - separator
  - `Copy download address`: copies d.url to the clipboard.
  - `Properties`: PropertiesDialog, refreshed on OK (MW:1085-1087).

### Drag and drop (MW:1496-1507)
- [ ] The whole window accepts URL or text drags. URLs come from the URL list or from text split on whitespace, filtered by URL_RE. — **2.x:** built (text/uri-list and text/plain); needs a manual check, synthetic drops are not run
- [x] Exactly 1 URL: `add_url(url, ask_address=False)`. More than 1: `_add_many`. — **2.x:** several URLs go through `act_add_many` and the question; one URL goes to the add flow (phase 6)

## 7. Details pane (details.py; MW:418-430, 1230-1252)
- [x] It sits under the list in a vertical splitter (object `detailsSplit`) with initial sizes `[420, details_height (default 210)]`. It cannot collapse. The height is persisted when the pane is visible (MW:1519-1520). — **2.x:** done
- [x] Visibility rule (MW:1241-1252): the pane shows only when `show_details` is on **and** a row is selected. Checked rows do not count. It shows the current row if that row is selected, else the first selected row. It hides when nothing is selected. — **2.x:** e2e `tests/e2e/run.sh`
- [x] Pane minimum is 560×170 (details.py:57, 119). — **2.x:** done
- [x] Header strip (object detailsTabs, 30 px): — **2.x:** done
  - tabs "Download status", "Connections", "Speed Limiter";
  - right-aligned faint hint "View → Details pane (F9)" (0.96×);
  - **Open in a window** button (icon open-window, 13 px), which emits open_window(id). The main window then calls `show_progress(id, force=True)`, which ignores the Options setting;
  - **close** button (tooltip "Hide the details pane"), which calls `set_details_visible(False)`.
- [x] Empty page: "Select a download to see its progress, connections and speed limit here." (faint, centred, wrapping). — **2.x:** done
- [x] Download status page (details.py:143-228): — **2.x:** `crates/lider-core/tests/view.rs` `details_pane`, e2e `tests/e2e/run.sh`
  - **Left block (300 px)**:
    - 52×52 tile holding a 30 px category icon.
    - Filename: bold, 1.125×, elided at the right.
    - URL without the scheme: mono, 0.92×, `link` colour (actx), elided; the tooltip is the full URL.
    - State line (muted 0.96×). The state is COMPLETE "Complete"; ERROR "Error: {error}"; PAUSED "Paused"; ASSEMBLING "Assembling…"; CONNECTING "Connecting…"; VERIFYING "Verifying {SHA-256 etc.}…"; otherwise running gives "Receiving data…", downloaded==0 gives "Queued", else "Paused". The line is "{state} · {category}", except "Complete · checksum verified" and "Checksum mismatch · {category}". It is elided in the middle.
    - Buttons (compact style, 30 px, pad 14, gap 6, icon 13): a state-dependent first button and **Open folder**. The first button is:
      - running: "Pause" (icon stop) → `manager.stop`
      - VERIFYING: "Skip check" (no icon) → `skip_check`
      - COMPLETE + mismatch: "Download again" → `redownload(overwrite=True)`
      - COMPLETE: "Open" → open the file
      - else: "Resume" (icon resume) → `manager.start`
    - Open folder follows the same complete/not-complete rule as the context menu (details.py:394-398).
  - **Middle**:
    - Large percentage: mono, 2×, bold, object bigNumber. "12.3%", or the downloaded size when the size is unknown.
    - "{downloaded} of {size}" (muted).
    - Progress bar 0..1000, 10 px high, no text.
    - Caption "Start positions and download progress by connections".
    - SegmentBar from dialogs/progress.
    - While verifying: the percentage becomes the verify %, the line becomes "{read} of {size} read", the bar shows the verify fraction and the caption becomes "Checksum progress — reading the merged file".
  - **Right stats grid (330 px, 2 columns)**: captions are faint, uppercase, 0.92× semibold; values are mono, bold, 1.08×:
    - TRANSFER RATE: rate, or "—" when not running.
    - TIME LEFT: duration, or "—".
    - DOWNLOADED: "2.39 / 5.68 GB". The unit is written once when both share it (`_of`, details.py:35-40).
    - CONNECTIONS: "{alive} active · {finished segs} done", or "—".
    - RESUME: Yes/No/Unknown, in okText colour.
    - CHECKSUM: "—" with no checksum; "{kind} · NN%" while verifying; "{kind} · verified|mismatch|skipped" once complete, coloured okText/badText; else "{kind} · pending".
  - **The stats column hides when the pane is narrower than 960 px** (details.py:58, 125-127).
- [x] Connections page: a list with columns `N.`, `DOWNLOADED`, `SPEED`, `INFO` (widths 50/120/110). It uses a small header, alternating rows and no root decoration. The INFO column uses InfoPillDelegate, which turns "Finished" into a teal pill. Rows are trimmed or added to match the connection count. Speed shows only while that connection is alive and the task is running. Info shows if the connection is alive or the task is running (details.py:364-371). — **2.x:** done
- [x] Speed Limiter page (per download): — **2.x:** `act_download_limit`
  - checkbox "Use Speed Limiter for this download";
  - "Maximum download speed:" spinbox 1..10,000,000, minimum width 110, then "KBytes/sec";
  - initialised when the download changes: checked if `speed_limit_kbps > 0`; value = its own limit or the global `speed_limit_kbps`;
  - any change applies straight away, `kbps` or 0 when unchecked, to the running task (`set_speed_limit`) and to `d.speed_limit_kbps` (details.py:261-262, 400-407).
- [x] The pane refreshes every tick while visible (MW:1354-1355). The selected tab is kept across downloads. — **2.x:** done

## 8. Status bar (MW:444-491)
- [x] It has no size grip and is visible by `show_statusbar`. It updates only while visible (every tick, MW:471-472). — **2.x:** done
- [x] Left side: — **2.x:** `crates/lider-core/tests/view.rs` `status_bar_and_tray`
  - `↓ {total rate}` in mono; "0 B/s" when idle.
  - `{n tasks} downloading · [{v} verifying · ]{q} in queue`, where q = downloads with a queue that are not COMPLETE/VERIFYING and not running.
- [x] Right side (permanent): — **2.x:** Speed Limiter and Clipboard done; Browser: … opens the Browser integration window (its window is built (phase 6)) and `recheck_browser` re-checks
  - `Speed Limiter: on|off`
  - `Clipboard: on|off`
  - **`Browser: connected|not set up`**: a link-style button (object sbLink, pointing hand, tooltip "Browser integration…"). Clicking it opens BrowserIntegrationDialog, then **clears the cached result and re-checks** (MW:464-469). Connected = any detected browser has the host installed. Exceptions count as not set up, and the result is cached.
- [x] Action status tips show here on hover (setStatusTip, MW:139). — **2.x:** the tip of the menu item or toolbar button under the mouse (or with the keyboard focus) replaces the left side of the status bar, like `QStatusBar.showMessage`; menu items no longer show tooltips (QMenu does not); e2e `toolbar and menu tips show in the status bar`. The GTK menu bar (D1) reports no hover

## 9. Tray icon (MW:493-509, 1357-1359, 1523-1575; app.py)
- [x] It is created only when `settings.tray_icon` is on and a system tray is available. Icon `app`. — **2.x:** `tray.rs`
- [x] Menu: `Open LID` (show_normal) · `&Add new download` · separator · submenu `Speed Limiter` (Turn on/Turn off, shared checkable items) · separator · `&Exit`. — **2.x:** done
- [ ] Left-click (Trigger) calls show_normal. — **2.x:** wired (`TrayIconEvent::Click`); appindicator may not deliver clicks, decision D2 needs a manual check on KDE/GNOME
- [x] Tooltip, updated every tick: `LID - {n} active download(s)`, or `LID` when idle. — **2.x:** `view::tray_tooltip`
- [x] Notifications: — **2.x:** desktop notifications (`tauri-plugin-notification`) with the same titles and texts
  - "Download failed" `{file}\n{msg}` (Warning), only when there is no progress dialog.
  - "Checksum mismatch" `{file}\nDownloading it again.` (Warning 4 s) on a changed event that carries a message.
  - "Checksum mismatch" `{file}\nThe file was kept.` (Warning 4 s).
  - "Download complete" `{file}` (Info 3 s) when no complete dialog is shown.
  - "LID" `"{queue}" processing stopped/finished` (Info 3 s).
  - "LID keeps downloading" `Open LID from this icon. Turn the icon off again in Options → Appearance.` (Info 5 s).
- [x] Close-to-tray: if a tray exists, `close_to_tray` is on and this is not a real quit, closing the window hides it (MW:1524-1527). — **2.x:** `app::close_requested`
- [x] Options → Appearance can turn the tray on or off live (MW:1181-1185). — **2.x:** Options OK re-applies the tray live (phase 6)
- [x] Start minimized (app.py:122): with `--minimized` or `settings.start_minimized`, **and** a tray exists, the window is not shown. Otherwise it is shown, and the menu-entry offer (`dialogs.menu.offer`) appears if `should_offer`. — **2.x:** hidden only when a tray exists; the menu offer is a message box

## 10. Search bar (Ctrl+F) and Find Next (F3)
- [x] The search bar is hidden at first and sits above the list (MW:378-383). Layout: "Find:" [edit with clear button] [**Find** (primary)] [Settings... (tooltip "LID Search settings")] stretch [✕ (tooltip "Close SearchBar")] (find.py:95-124). — **2.x:** done
- [x] show_searchbar: shows the bar, puts the current query in the field, focuses it and selects all (MW:1113-1117). — **2.x:** e2e `tests/e2e/run.sh`
- [x] Find (button or Return), MW:1119-1128: — **2.x:** e2e `tests/e2e/run.sh`
  - query = text; active = the query is non-empty;
  - validate, and on an error show info "LID Search settings" with the message;
  - persist `search_flags` and `search_string`;
  - reset Find Next and refresh, which filters the list.
  - Matching is case-insensitive (the test uses "LECTURE").
- [x] Settings...: FindDialog titled "LID Search settings" without the query field. On OK it persists the flags and refreshes if the search is active (MW:1130-1134). — **2.x:** its window is built (phase 6)
- [x] Close (✕ or Esc): hides the bar, sets the search inactive (the filter is removed but the query is kept), refreshes and focuses the list (MW:1136-1140). — **2.x:** e2e `tests/e2e/run.sh`
- [x] Find Next (MW:1142-1153): — **2.x:** e2e `tests/e2e/run.sh`; with no query it opens the Find window (its window is built (phase 6))
  - with no query, opens the FindDialog first and stops on cancel; otherwise persists;
  - `finder.next(rows)` finds the next matching row index after the last one found;
  - with no match, shows info "Find file" / "No such files found";
  - otherwise selects that row only (ClearAndSelect) and scrolls to it.
  - It works while the bar is closed. The test expects rows 0 and then 2.
- [x] A tree selection change resets the finder. — **2.x:** done

## 11. Tick loop (500 ms) and event handling (MW:1350-1402)
- [x] tick: — **2.x:** `pump.rs`
  1. `handle_events(manager.poll())`
  2. `refresh_rows()`
  3. `update_tree_counts()`
  4. refresh the details pane if visible
  5. update the status bar
  6. update the tray tooltip.
- [x] handle_events, per event: — **2.x:** `app::handle_events`
  - `complete` → `_download_complete`.
  - `error` → if a progress dialog is open, refresh it; else the tray shows "Download failed".
  - `mismatch` → `_checksum_mismatch(id, ask = message != "kept")`.
  - `changed` with a message → tray "Checksum mismatch … Downloading it again."
  - `queue_done` (queue still exists) → `_queue_done`.
  - `checked` and `queue_started` have no specific handling.
  - If there were any events, refresh the rows.
- [x] _download_complete: — **2.x:** rules done (progress options, running queue, notification, power off, exit); the Complete dialog is its window is built (phase 6)
  - Options start as `{show_complete: settings.show_complete_dialog, exit: False, shutdown: False}`. If a progress dialog exists, its `completion_options()` replace them and the dialog closes.
  - If the download belongs to a running queue, show_complete is forced False.
  - show_complete: a parentless, non-modal CompleteDialog. When it is closed with "don't show" ticked, `show_complete_dialog=False` is saved.
  - Otherwise the tray shows "Download complete".
  - Then shutdown → `shutdown_computer`, or exit → `quit_app`.
- [x] _checksum_mismatch: — **2.x:** its window is built (phase 6)
  - closes the progress dialog;
  - with ask False, shows the tray "kept" message and stops;
  - otherwise a non-modal MismatchDialog (deleted on close). On accept, `_mismatch_choice` runs:
    - "again" → `redownload(id, overwrite=True)` + show_progress;
    - "delete" → move the file to the **Trash** (QFile.moveToTrash) if it exists, remove the download and uncheck it;
    - then refresh.
- [x] _queue_done: — **2.x:** `app::handle_events`
  - tray message;
  - `schedule.open_when_done`: open that path;
  - `shutdown_when_done` → shutdown, else `exit_when_done` → quit.
- [x] shutdown_computer (MW:1457-1476): — **2.x:** message box with the 30 s countdown and Cancel, then `systemctl poweroff`; needs a manual check
  - Warning box "Shut down" with "LID has finished downloading. The computer will be turned off in N seconds." It counts down from 30 each second and has a Cancel button.
  - Cancel stops it. At 0, `manager.shutdown()` runs and then `systemctl poweroff`.

## 12. Adding downloads (main-window flows)
- [x] **add_url(url=None, referer, description, parent_page, ask_address=True, headers, filename)** (MW:782-838): — **2.x:** its window is built (phase 6)
  1. If url is None, or `ask_address` is set and the url is empty, open AddUrlDialog prefilled with the url. Its result gives url plus auth credentials.
  2. Duplicates (`manager.find_duplicate`) follow `settings.duplicate_action`, which is ask|number|overwrite|complete:
     - "ask" opens DuplicateDialog. Cancel aborts. Its "remember" checkbox persists the choice.
     - "complete": an already COMPLETE duplicate shows CompleteDialog; otherwise the duplicate is started and its progress shown, and the add stops.
     - "overwrite" replaces the old entry.
     - "number" falls through to a unique name.
  3. A YouTube URL goes to the YouTube flow.
  4. Otherwise a busy cursor runs an async `manager.probe(url, referer, user, pw, headers)`.
     - On error, ask `Cannot get information about the file:\n{error}\n\nAdd it anyway?`. On Yes the name is `safe_filename(filename or filename_from_url)`, size −1 and resumable unknown.
     - On success, the browser-supplied filename wins when it has an extension and the probed name does not.
  5. `_confirm_and_add` follows.
- [x] **_confirm_and_add** (MW:894-922): — **2.x:** its window is built (phase 6)
  - start = `!only_add_to_queue`;
  - the category and save_dir come from `categories.classify`;
  - unless replacing, the name is made unique (`stem (2).ext`, … against both list entries and files on disk, MW:885-892);
  - if `show_start_dialog`, FileInfoDialog runs (path, category, description, checksum; action "later" queues to r.queue and does not start, otherwise it starts) and settings are saved;
  - replace: delete the old file if it is complete and at the same path, then remove the old entry;
  - `manager.add(...)`, refresh, and show_progress if started.
- [x] **YouTube** (MW:840-883): — **2.x:** its window is built (phase 6)
  - Without yt-dlp: warning "Downloading from YouTube needs the yt-dlp package:\n\n    pip install yt-dlp".
  - Async probe with a busy cursor. Errors show "Cannot read this YouTube video:\n{err without 'ERROR: '}". No variants shows "This YouTube video has no downloadable formats."
  - FileInfoDialog with media_info, preselected `media_kind`/`media_quality`. The chosen kind and quality are remembered and saved.
  - The replace logic is the same as above. The download is added with `parent_page=url`, resumable, a media record, and a queue only when not started. Progress is shown if started.
- [x] **_add_many** (batch, drop, import; MW:1010-1019): each URL gets a safe name and a classified category, a unique name, and is added to MAIN_QUEUE with credentials. Then the window refreshes and asks `{n} file(s) were added to the "Main download queue". Start the queue now?`. Yes starts the queue. — **2.x:** `crates/lider-core/tests/view.rs` `unique_names_and_add_many` and the start question
- [x] **_add_choices** (LinksDialog results; MW:973-980): each choice is added with a unique name to MAIN_QUEUE with referer, parent and headers. If there were any, the Main queue **starts automatically**. — **2.x:** its window is built (phase 6)
- [x] **Grabber** (MW:940-971): — **2.x:** its window is built (phase 6)
  - QInputDialog "Grabber" / "Enter the address of the web page to explore:", prefilled with the clipboard if it holds a URL, else "https://". An invalid input aborts.
  - Async fetch without a range, up to 8 MiB, decoded with the response charset (default utf-8, replacement on errors).
  - Error: warning "Cannot open the web page:\n{err}". No links: "Cannot find the links!".
  - LinksDialog → `_add_choices(referer=final URL, parent=final URL)`.
- [x] **remove_selected** (MW:1052-1068): — **2.x:** e2e `tests/e2e/run.sh`
  - Question box "Confirm deletion of downloads": "Are you sure you want to delete {n} file(s) from the download list?" with Yes/No and the checkbox "Also delete the downloaded files from disk".
  - Yes → `remove(ids, delete_files)`, uncheck them, refresh.
- [x] **delete_completed** (MW:1070-1076): if n > 0, ask "Remove all {n} completed file(s) from the list?\nThe files stay on the disk." and on Yes call `remove_completed`. — **2.x:** done
- [x] **show_progress(id, force)** (MW:1022-1032): one ProgressDialog per download, reused, shown and raised. It is skipped when `show_progress_dialog` is off, unless forced. — **2.x:** `commands::show_progress`; the window is its window is built (phase 6)

## 13. Import / Export (MW:1301-1347)
- [x] Export .ef2: save dialog "Export download list", default "downloads.ef2", filter "Export files (*.ef2)". CRLF line endings, downloads sorted by id, written as `<`, url, optional `referer: X`, `>`. — **2.x:** `lider_core::ef2`; test `ef2_round_trip_format`
- [x] Export text: default "downloads.txt", "Text files (*.txt)". One URL per line plus a trailing newline. — **2.x:** done
- [x] Import .ef2: open dialog "Import", filters "Export files (*.ef2);;All files (*)". `parse_ef2` reads `<…>` blocks: the first line is the url, and a `referer:` line is matched case-insensitively. Then `_add_many`. — **2.x:** done
- [x] Import text: "Text files (*.txt);;All files (*)". Whitespace-separated tokens matching URL_RE go to `_add_many`. — **2.x:** done

## 14. Browser requests, clipboard, single instance
- [x] **handle_browser_request(req)** (MW:983-1008): — **2.x:** `app::browser_request`; the add and links flows it starts are its window is built (phase 6)
  - `action:"show"` → show_normal.
  - Headers come from `cookies` → `Cookie` and `user_agent` → `User-Agent`. `page = page_url`; `referer = referer or page`.
  - `download`: only when the url matches URL_RE (so `javascript:` is ignored). Calls `add_url(url, referer, parent_page=page, ask_address=False, headers, filename)`.
  - `links`: items that are dicts with a valid url become `(url, text, kind default "a")`. If any remain, LinksDialog (raised) → `_add_choices(referer, page, headers)`, which auto-starts the Main queue.
- [x] **Clipboard monitoring** (MW:1479-1494): — **2.x:** `app::clipboard_text` (arboard, polled each tick); needs a manual check
  - Only when `monitor_clipboard` is on. The text is stripped.
  - It is ignored when it equals the last captured value or does not match URL_RE. Otherwise it is remembered.
  - It is skipped when the host ends with any whitespace-separated entry of `no_auto_sites` (case-insensitive).
  - YouTube → `add_url(ask_address=False)`. Otherwise, if the filename from the URL is an auto-capture type (`categories.is_auto_capture_type`) → `add_url(ask_address=False)`.
- [x] **Single instance** (app.py:101-112): IPC lines starting with `{` are parsed as JSON and passed to handle_browser_request; invalid JSON is ignored. Any other line calls show_normal, and anything other than `--show` is also treated as a URL: `add_url(line, ask_address=False)`. Command-line URLs are added after start-up. — **2.x:** `app::on_line`
- [x] show_normal = showNormal + raise + activate (MW:1510). — **2.x:** done

## 15. Close behaviour (MW:1515-1575)
- [~] save_settings: geometry (base64), sidebar width if visible, details height if visible, then write settings. — **2.x:** geometry as `window_state`; sidebar width and details height as 1.x
- [x] closeEvent: — **2.x:** `app::close_requested`
  1. close-to-tray rule (§9);
  2. otherwise `_confirm_close()`, after which `really_quit` resets;
  3. "keep" → ignore;
  4. "background" → `_to_background`, then ignore;
  5. "stop" → save settings, close all progress dialogs, `manager.shutdown()`, accept, and quit the app.
- [x] _confirm_close: with nothing running and nothing verifying, the result is "stop". Otherwise CloseWhileDownloadingDialog(running list, background option, exiting=really_quit), parented to the window if visible. The background option is offered unless this is an Exit **and** a tray exists, so Exit from the tray only offers stop or stay. — **2.x:** a message box with keep / background / stop; the designed dialog is phase 6
- [x] _to_background: sets `tray_icon=True` and builds the tray if needed, then saves. With no tray available the window minimizes instead. Otherwise it hides and shows the "LID keeps downloading" message. — **2.x:** `app::to_background`

## 16. Options and appearance hooks
- [x] open_options(tab): selects the tab whose text equals `tab`, so "Appearance" opens on that tab; plain Options opens "General". On OK it runs `apply_appearance`, saves, rebuilds the tree and refreshes (MW:1156-1165). — **2.x:** its window is built (phase 6)
- [x] apply_appearance (MW:1170-1185) re-applies: — **2.x:** the view applies appearance settings on every tick
  - theme, toolbar labels, menu bar mode;
  - show categories, details, status bar;
  - list font, falling back to the app font;
  - tray on/off.

## 17. Theme system (theme.py)
- [~] Every theme runs on Fusion behind a proxy style that hides shortcut underlines (theme.py:332-347). Each theme supplies a palette and a generated stylesheet. `apply(name)` falls back to ember for unknown names, skips the work if the sheet is unchanged, and then recolours icons with `ink` and `icon_ac`. — **2.x:** CSS on the same tokens instead of Fusion, a palette and QSS
- [x] Theme names: `ember`, `ember-dark`, `dark`. Titles: "Ember", "Ember Dark", "Dark". The dark ones are ember-dark and dark. — **2.x:** done
- [x] resolve (theme.py:74-78): with `follow_system`, use `settings.dark_theme` (default ember-dark) when the system scheme is Dark, else "ember". Otherwise use `settings.theme`, falling back to ember. — **2.x:** `app::resolve_theme`
- [x] The app re-resolves when the system scheme changes. — **2.x:** portal `color-scheme`, polled every 2 s
- [x] choose_theme("system") sets `follow_system=True`; any other key sets follow_system False and `theme=key`. Then it applies and saves. — **2.x:** e2e `tests/e2e/run.sh`
- [x] Defaults: theme ember, follow_system True, dark_theme ember-dark, density comfortable. — **2.x:** done

### Colour tokens (theme.py:20-36)
| token | ember | ember-dark | dark |
|---|---|---|---|
| win | #F7F5F2 | #1A1714 | #1B1E24 |
| pane | #FFFFFF | #141210 | #15181D |
| alt | #FBFAF8 | #181512 | #181B21 |
| hdr | #F9F7F4 | #1D1915 | #1D2027 |
| line | #E7E2DA | #2E2925 | #2B3039 |
| line2 | #D8D1C6 | #40382F | #3A404B |
| tx | #2A2420 | #EDE6DF | #E2E8F0 |
| mu | #6B625A | #AEA399 | #A3ADBD |
| fa | #9A9087 | #7C7168 | #6E7787 |
| ac | #EA580C | #F97316 | #F97316 |
| acsel | #FDEEE4 | #3A2417 | #33261F |
| actx | #C2410C | #FDBA74 | #FDBA74 |
| ok | #0D9488 | #2DD4BF | #2DD4BF |
| bad | #DC2626 | #F87171 | #F87171 |
| track | #EFEAE3 | #2B2520 | #2A2F38 |
| input | #FFFFFF | #100E0C | #111318 |
| btn | #FFFFFF | #221E1A | #252932 |
| ink | #1E293B | #E2E8F0 | #E2E8F0 |
| icon_ac | #EA580C | #F97316 | #F97316 |
| title | #EEEAE4 | #221E1A | #23262D |

### Palette mapping (theme.py:316-329)
- Window=win; WindowText=tx; Base=pane; AlternateBase=alt; Text=tx; Button=btn; ButtonText=tx; ToolTipBase=pane; ToolTipText=tx; Highlight=ac; HighlightedText=#FFFFFF; PlaceholderText=fa; Link=actx; Mid=line2; Dark=line2; Light=pane; Midlight=line.
- Disabled Text, ButtonText and WindowText use fa.

### Generated glyphs (theme.py:86-104)
- 24-unit SVGs with round caps:
  - check and dash: white, stroke 3;
  - down, up, right: fa, stroke 2.4;
  - close: fa, stroke 2.2.
- They are used for checkbox indicators, menu check marks, combo/spin/header arrows and tree branches.

### QSS rules to replicate (theme.py:114-313)
- **General**: no focus outline. Window/dialog bg win. Tooltip: pane bg, line2 border, radius 6, padding 4/8.
- **Menu bar and menus**:
  - Menu bar items: padding 5/10, radius 6, selected acsel/actx.
  - Menus: pane bg, line2 border, radius 10, padding 6.
  - Menu items: padding 6/28/6/12, radius 7, selected acsel/actx, disabled fa.
  - Separator: line, 1 px, margin 4/6.
  - Checked indicator: 14 px, ac bg, radius 4, white check.
- **Toolbar**: win bg, bottom border line, padding 8/12, spacing 8. Separators are 1 px line2 with margin 6/4.
- **Toolbar buttons**:
  - Generic: radius 9, hover acsel.
  - pill/danger: pane bg, line2 border, radius 17, weight 600. pill hover border ac. Disabled: win bg, fa text, line border.
  - danger: bad text, hover border bad. Menu-button zone 31 px with no arrow image; the chevron is painted instead.
  - primary: ac bg, white text, radius 19, weight 700, hover actx.
  - ghost: weight 600, radius 9.
- **Chip**: acsel bg, radius 15; label actx, weight 700; close hover bg ac.
- **compact button**: btn bg, line2 border, radius 7, weight 600, hover border ac, disabled fa/win/line.
- **Segmented control**: frame track, radius 15; seg buttons radius 12, padding 4/13, mu text; checked pane bg, tx text.
- **QPushButton**:
  - btn bg, line2 border, radius 16, padding 6/14, min-height 18, weight 600, hover border ac.
  - `primary=true`: ac bg/border, white text, hover actx. Disabled: fa/win/line. `#small`: padding 5/0, radius 10.
  - Menu indicator: the down glyph.
  - `#ghostBtn`: transparent, mu text, hover tx. `#dangerBtn`: bad text, hover border bad.
- **Inputs** (line edits, text edits, spin boxes, combo boxes, date/time edits):
  - input bg, line2 border, radius 8, padding 4/9, min-height 20, selection ac/white; focus border ac; disabled fa/win.
  - Combo: `mono=true` uses the fixed font; drop-down 22 px; down arrow 12 px. Its popup: pane bg, radius 8, padding 4, selection acsel/actx.
  - Spin/time/date buttons 18 px with up/down arrows 10 px.
  - `invalid=true` line edit: bad border.
- **Checkbox/radio**:
  - 14 px boxes, radius 4, line2 border, input bg; hover border ac.
  - Checked or indeterminate: ac fill with white check or dash.
  - Disabled: win/line. Checked-disabled: acsel.
  - Radio: radius 8; checked shows a radial gradient (white centre to 36%, ac from 42%).
- **GroupBox**: pane bg, line border, radius 10, margin-top 22, padding 12/10/10. Title: fa, weight 700.
- **Tabs**: tab text mu, padding 8/12, weight 600; selected tx with a 2 px ac underline; hover tx. Tab pane: top border line.
  - In detailsTabs and tabStrip: padding 6/12/5 (detailsTabs) and 8/12/6 (tabStrip), with a transparent 2 px underline that turns ac when selected.
  - `#optionsTabs` tab bar offset 14.
- **Item views**:
  - pane bg, alternate alt, line border, radius 9.
  - Items: padding 2/4; selected acsel bg with tx text; hover alt.
  - `#downloads`: no border or radius, item padding 0/6.
  - `#categories`: win bg, no border, padding 0/6. Items radius 6, padding 3/4; selected acsel/actx, weight 600.
  - `#queues`: padding 6, items radius 6, padding 4; selected acsel/actx.
  - `#checkList`: padding 6, items radius 7.
  - Branch arrows use the right/down glyphs.
- **Header**: hdr bg; sections mu text, right and bottom borders line, padding 6/8, weight 700. `smallHeader=true`: weight 600, no right rule, padding 0/10, min-height 27. Sort arrows use the glyphs.
- **Scrollbars**: 11 px, transparent track; handle line2, radius 4, minimum 28; no arrow buttons.
- **Progress bar**: track bg, radius 6, text hidden; chunk ac, radius 6.
- **Splitters**: handle line, 1 px. `#sidebar` handle 6 px win bg with a 1 px right border line, 2 px ac on hover. `#detailsSplit` handle 6 px pane bg with a 1 px bottom border line2, 2 px ac on hover.
- **Status bar**: win bg, top border line, mu text; labels padding 0/8; sbLink mu, ac on hover.
- **Dialog parts**:
  - footer: win bg, top border line; footer buttons min-width 62.
  - formLabel: mu.
  - noteBox: track bg, mu text, radius 9, padding 9/12.
  - Pills: radius 11, padding 3/10. ok: ok@0.14 bg, ok text. bad: bad@0.12, bad text. accent: acsel/actx. muted: track/mu.
  - Text styles: sectionTitle fa, weight 700, letter-spacing 1; muted mu; faint fa; dialogTitle and bigNumber weight 700; link actx; okText ok; badText bad; errorText bad.
  - panel: pane bg, line border, radius 10.
  - Tiles: radius 12, small 10, large 18. Inside details the tile bg is win. Details bg pane. detailsTabs: hdr bg, bottom border line.
  - Badges: okBadge ok@0.13 radius 21; badBadge bad@0.12; accentBadge acsel.
  - mismatchBox: bad@0.07 bg, bad@0.35 border, radius 10. matchBox: the same with ok@0.08/0.35.
  - themeCard: pane, line border, radius 10. Selected: acsel bg with a 2 px ac border. Its radio is weight 600.
  - roundBtn: btn bg, line2 border, radius 15.
  - nextRun: acsel bg, radius 9, labels actx.
  - tagInput: input bg, line2 border, radius 9; focused border ac.
  - tag: acsel bg, actx text, radius 13, padding 3/4/3/10, weight 600, hover border ac.

### Icons and recolouring (icons.py)
- [x] SVGs are drawn with ink `#1E293B` and accent `#2563EB`. At runtime the ink becomes the theme's `ink` and the accent becomes its `icon_ac`. The SVGs are rendered at 16/20/24/32/48/64 px times the device pixel ratio (icons.py:22-24, 77-88). — **2.x:** `ui/src/lib/icons.ts`
- [~] A missing name falls back to the desktop theme icon, then to the generic file icon. Aliases: download and downloads → folder-download; folder-open → folder. — **2.x:** aliases as 1.x; a missing name falls back to the folder icon (no desktop icon theme in the webview)
- [~] `bind(obj, name)` registers a widget so it is re-iconed on a theme change; re-binding replaces the name. `category_icon(name)` returns `category-<lower>` or `folder`. `file_icon(filename)` uses the system file icon provider. — **2.x:** `category_icon` as 1.x; the system file-type icons of `file_icon` are not used
- [x] Not recoloured, with fixed colours: add-white and play-white (#FFFFFF); finished and shield-check (#0D9488); delete and shield-alert (#DC2626); app (#1D4ED8 and #FFFFFF). — **2.x:** done
- [x] `lider/ui/assets/icons/` contains: add-url, add-white, app, category-compressed, category-documents, category-music, category-programs, category-video, checksum, clock, close, connection, delete-completed, delete, down-arrow, filetypes, finished, folder-download, folder, general, grabber, info, link, open-window, options, play-white, proxy, queue, queue-sync, remove, resume, saveto, scheduler, search, shield-alert, shield-check, speed, start-queue, stop-all, stop-queue, stop, unfinished, up (43 SVGs). — **2.x:** all 43 built in

## 18. widgets.py building blocks
- [x] `mono_font(base)`: the system fixed font, at 0.95× the base size when a base is given (40-44). — **2.x:** `system::mono_family` (fontconfig)
- [x] `dialog_header(icon, title, subtitle, tile, badge)` (58-95): — **2.x:** the message box header is done; dialog headers come with its window is built (phase 6)
  - Icon: bare 30 px, or 22 px on a 40 px tileSmall, or 22 px on a 42 px `{badge}Badge` circle.
  - Title: bold, 1.25×, object dialogTitle. Subtitle: muted, wrapping.
- [x] `shell(dialog, icon, title, subtitle, spacing=10, tile, badge)` (99-121): the frame of every dialog. Body margins 18/16/18/14 with the optional header on top. Footer bar (object footer) with margins 16/12/16/12 and spacing 8. Returns (body, footer). — **2.x:** its window is built (phase 6)
- [x] `panel(spacing=12)`: a rounded card, object panel, margins 14 (124-131). — **2.x:** its window is built (phase 6)
- [x] `caption(text)`: uppercase sectionTitle at 11/12 size (134-140). — **2.x:** `.sectionTitle`
- [x] `link_field(edit)`: mono font with a leading link icon (143-147). — **2.x:** its window is built (phase 6)
- [x] `form_label` (object formLabel, optional fixed width) and `form_row(label, *items, width=96)`: a fixed muted label column, then the controls; the last control stretches (150-169). — **2.x:** its window is built (phase 6)
- [x] `note(text, boxed)`: wrapping faint text, or a noteBox with boxed=True (172-176). — **2.x:** its window is built (phase 6)
- [x] `pill_label(text, kind)` and `set_pill`: QLabel pills ok/bad/accent/muted at 11.5/12 size, DemiBold (179-197). — **2.x:** its window is built (phase 6)
- [x] `tab_strip(names)`: underlined, non-expanding tabs on a strip with a bottom rule, margins 14 (200-213). — **2.x:** details tabs done; the dialogs' tab strip is its window is built (phase 6)
- [x] `small_header(view)`: header font 10.5/12 with the smallHeader property (216-222). — **2.x:** `table.small`
- [x] `primary(button)`: sets `primary=true`, which paints the accent button (225-228). — **2.x:** `.btn.primary`
- [x] `add_header`: inserts a dialog_header above an existing grid or vertical layout (231-245). — **2.x:** its window is built (phase 6)
- [x] `FlowLayout`: lays items out and wraps them, spacing 6, margins 8, height-for-width (249-319). — **2.x:** its window is built (phase 6)
- [x] **TagInput** (336-484): — **2.x:** its window is built (phase 6)
  - Values are removable chips, normalised and deduplicated.
  - Enter, Return or "," commits. Pasted text is split on commas and whitespace. Backspace on an empty field removes the last chip. Focus-out commits, and focus toggles the `focused` style.
  - Each chip is a ToolButton "tag" with a trailing 11 px close icon (right-to-left layout), tooltip "Remove {v}"; clicking removes it.
  - Clicking the frame focuses the edit. The edit's minimum width is max(120, placeholder width + 24).
  - `values()` commits pending text first. `text()` joins with spaces; `set_values` accepts a string or a list.
  - `meta_row()`: "Enter or , adds · Backspace removes the last" … "{n} {noun}" plus a ghost "Clear all" button.
  - `changed` signal.
  - Normalisers: `extension_tag` strips, removes leading dots and lowercases; `site_tag` strips and lowercases.
- [x] `_box` checkbox painting (496-514): rounded radius 4. Unchecked: input fill, line2 1.5 px border. Checked: ac fill with a white 2 px tick at (.27,.52)→(.43,.68)→(.74,.33). Partial: white dash from .3 to .7 at .5. — **2.x:** `.box`
- [x] `pill()`: described in §6. — **2.x:** done
- [x] `check_box_pixmap`, `cross_pixmap` (stroke 2.4/24) and `check_pixmap` (stroke 2.2/24): helper pixmaps. — **2.x:** inline SVG
- [x] **ToolButton** (557-615): sizes itself from padding, icon, gap and text, plus an 11 px chevron (stroke fa, 1.2 px) for split menus. It paints its own icon and label so the gap is exact. Disabled state uses the disabled palette and icon mode. — **2.x:** `.tb`
- [x] **ChecksumEdit** (632-711): — **2.x:** its window is built (phase 6)
  - Algorithm combo: "Auto-detect" plus checksum titles, width 124.
  - Mono hash field with placeholder "Paste a hash (optional)". The detected type shows as an accent pill inside the right edge of the field, and the text margin reserves room for it.
  - Message line: hint "Optional — LID checks the file when the download finishes.", or the error in errorText style. An error also sets `invalid=true` on the field.
  - `value()` raises while invalid. `setValue("sha256:hex")` resets the combo to Auto-detect.
- [x] **CheckChip** (731-774): described in §4. Height 30, margins 10/0/6, spacing 8. A 14 px partial box icon, the label, and a 20×20 ✕ (12 px cross in actx, white when hovered or active, tooltip "Clear the checks", pointing hand). It repaints on a style change. — **2.x:** e2e `tests/e2e/run.sh`
- [x] **CheckHeader** (778-798): tri-state box in section 0. A click in section 0 emits toggled and does not sort. — **2.x:** e2e `tests/e2e/run.sh`
- [x] **TypePillDelegate** (801-832): text shown as a muted track pill (height ≤ 22, 11.5/12 DemiBold, centred); row height 27. — **2.x:** its window is built (phase 6)
- [x] **PillDelegate(pills)** (835-879): mapped texts become pills. "ok" pills are ok on ok@31 with a 12 px check mark; other kinds are mu on track. Unmapped text is drawn in mu and elided. Row height 27. — **2.x:** its window is built (phase 6)
- [x] **InfoPillDelegate**: "Finished" becomes a teal ok pill (882-884). — **2.x:** `.infopill`
- [x] **ListDelegate** (887-935) and **TreeDelegate** (938-962): described in §6 and §5. The tree badge is mono, bold, 0.82×, width = text+14, height = font height+2, 6 px from the right, rounded. Selected: ac bg, white text. Unselected: track bg, mu text. — **2.x:** done
- [~] Roles: STATUS_ROLE = UserRole+10, COUNT_ROLE = UserRole+11, ID_ROLE = UserRole+1. — **2.x:** no Qt roles: each row carries its data

## 19. util.py
- [x] `run_async(fn, cb)`: runs fn on a daemon thread and calls `cb(result, error)` on the UI thread (42-53). A Tauri port maps this to an async command plus an event. — **2.x:** async Tauri commands
- [x] `open_path`: open the path with the desktop default (QDesktopServices). — **2.x:** `system::open_path` (xdg-open)
- [x] `open_folder(path)`: DBus `org.freedesktop.FileManager1.ShowItems` with the file URI, which selects the file. On OSError it opens the parent folder (60-70). — **2.x:** `system::open_folder` (FileManager1.ShowItems, else the folder)
- [x] Also `dlu(w,h)` (×1.5, ×1.625), `separator()` (HLine) and `page_header(icon, title)`: a 22 px icon with a bold title at 14/12, the header of each Options page. — **2.x:** its window is built (phase 6)

## 20. What each UI test asserts (tests/test_ui.py)
- `test_menu_bar_layout` (55): the top-level menus are exactly Tasks/File/Downloads/View/Help. Downloads contains Pause All, Stop All, Delete All Completed, Find (Ctrl-F), Find Next (F3), Scheduler, Start queue, Stop queue, Speed Limiter and Options.
- `test_toolbar_captions` (64): toolbar captions are exactly Add URL, Start, Stop, Delete, Options, Scheduler, Grabber.
- `test_category_tree` (68): the top nodes are All Downloads, Unfinished, Finished, Grabber projects, Queues. The All Downloads children are Compressed, Documents, Music, Programs, Video (General hidden).
- `test_searchbar_filter_and_find_next` (75): searching "LECTURE" filters the list to the two lecture files, case-insensitively. Closing the bar restores all 3. Find Next selects row 0, then row 2.
- `test_tree_filters` (91): Finished shows only the complete item; All/Compressed shows the zip; Queue/Main shows the queued item.
- `test_ef2_round_trip_format` (106): parse_ef2 reads CRLF blocks into (url, referer) with an empty referer when absent.
- `test_download_through_window` (111): a real 2 MiB download completes through the tick loop, the file exists, and the status text is "Complete".
- `test_browser_download_request_reaches_add_url` (122): a download request calls add_url with referer = page_url, parent_page, ask_address=False, Cookie and User-Agent headers and the filename. A `javascript:` URL is ignored.
- `test_browser_links_request_adds_to_queue_with_cookies` (132): a links request adds to the Main queue with the Cookie header and referer = page, and starts the Main queue.
- `test_remove_selected_asks_and_removes` (152): answering Yes in the confirmation removes the selected download.
- `test_checked_rows_drive_start_stop_delete` (165): with no checks, targets = selection. With checks, targets = checked, the header is PartiallyChecked and the chip says "2 checked". toggle_all checks all, then unchecks all. setData(CheckStateRole) checks one row.
- `test_theme_choice_switches_palette` (182): choose_theme("dark") makes the current theme dark and turns follow_system off; choosing ember switches back.
- `test_menu_bar_modes` (192): custom gives a non-native menu bar with the inline bar visible; native hides the inline bar; both shows the inline bar only when a global menu is available. The inline bar has the same menu titles as the main bar.
- `test_view_menu_ends_with_appearance` (205): the last View item is Appearance…. It opens Options on the "Appearance" tab, while Options opens on "General".
- `test_details_pane_follows_the_highlighted_row` (217):
  - The pane is hidden with no selection, and checking a row does not show it.
  - Selecting a row shows the pane for that download.
  - open_window opens a progress dialog.
  - Toggling show_details hides and shows the pane, and clearing the selection hides it.
- `test_checksum_states_and_mismatch_choices` (243): status_info gives verified/"Verified" and mismatch/"Checksum mismatch". A mismatch event opens MismatchDialog. "again" calls redownload(overwrite=True). "delete" moves the file to the trash and removes the download.
- `test_closing_with_running_downloads_asks_first` (272): with a running download, "keep" leaves the window open; "background" ignores the close, turns tray_icon on and hides or minimizes the window without shutting down; "stop" accepts and shuts down once. The dialog was asked 3 times.
- `test_browser_status_opens_browser_integration` (315): the status text starts with "Browser: ". Clicking opens the dialog, and the status is re-checked afterwards ("Browser: not set up").
- `test_sidebar_widens_and_its_width_is_kept` (330): the sidebar can be dragged to 560 and save stores sidebar_width=560. Dragging it too far still keeps the right side at least 560.
- `ChecksumFieldTests.test_invalid_checksum_holds_start_until_fixed` (343): a 62-character hash disables "&Start Download" and shows a "got 62" message. A SHA256SUMS line ("hash  name") is accepted with the pill "SHA-256", and the result is `sha256:<hex>`.
- `CheckHashDialogTests.test_compute_match_mismatch_and_when_finished` (366):
  - can_check is True for complete, False for fresh and True for partial downloads.
  - With an empty field the button reads "Compute hash" and computes without saving.
  - A matching hash gives the "match" state, sets verified and stores the checksum.
  - A mismatch with "save" unticked leaves the stored result alone.
  - On a partial download the state is "running", and "later" stores the checksum with checksum_force=True.
- `TagInputTests.test_enter_comma_paste_and_backspace` (415): normalisation and deduplication ("AVI .mkv avi" → avi, mkv), comma/space splitting on commit, and Backspace-on-empty removing the last value.
- `BrowserDialogTests.test_get_the_extension_links_point_at_release_files` (428): the dialog's links include RELEASES_URL, and every `/download/` link names a file the extension build actually produces.

## 21. Behaviours that are easy to miss in a port
- Pause All and Stop All do the same thing, but only Stop All has an enabled-state rule (MW:163-164).
- The Q column shows only an icon and has no text. The Status column has no DisplayRole text, only the pill.
- Sorting by rate uses bytes downloaded, and sorting by time left uses remaining bytes, not live values.
- The Queues root node lists every queued download. Grabber projects is always empty.
- Checks survive refreshes but are pruned to the visible rows when the filter changes.
- Resume opens a progress window only for a single target with the details pane off.
- Double-click and Resume respect `show_progress_dialog`; the details "Open in a window" button forces it.
- Links from the grabber, clipboard batch or browser auto-start the Main queue. Batch, drop and import ask first.
- The window title and the tray tooltip use different text; the tray tooltip uses "LID".
- Settings are saved on close, theme change, limiter change, category edits, Options OK and some dialogs. Sort, column widths and font changes are only saved later, on the next save.