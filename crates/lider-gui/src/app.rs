//! The running program: the download manager, what the user has selected, the 500 ms tick that polls the
//! manager and rebuilds what the window shows (the view model of `lider_core::view`), and every action of
//! the menus, the toolbar and the context menus (`main_window.py` in 1.x).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use lider_core::manager::DownloadManager;
use lider_core::menu::Menu;
use lider_core::models::{MAIN_QUEUE, Status};
use lider_core::paths::Dirs;
use lider_core::search::{FindNext, SearchSettings};
use lider_core::view::{self, ActionState, Details, Row, StatusBar, TreeFilter, TreeNode};
use lider_sys::platform::prelude::*;

use crate::boxes::{Answer, MsgBox, Then};
use crate::theme::{Palette, ThemeName};

pub const TICK: Duration = Duration::from_millis(500);

/// A visible column: its key and saved width.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub key: String,
    pub width: f32,
}

/// What the window shows, computed from the manager.
#[derive(Default)]
pub struct View {
    pub tree: Vec<TreeNode>,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub status: Option<StatusBar>,
    pub actions: Option<ActionState>,
    pub menus: Vec<Menu>,
    pub details: Option<Details>,
    pub queues: Vec<String>,
}

pub struct App {
    /// The engine runs on this runtime; it lives as long as the window.
    #[allow(dead_code)]
    pub rt: tokio::runtime::Runtime,
    pub manager: DownloadManager,
    pub dirs: Dirs,
    pub filter: TreeFilter,
    pub search: SearchSettings,
    pub finder: FindNext,
    pub search_open: bool,
    pub search_text: String,
    /// Put the keyboard focus in the search field on the next frame.
    pub focus_search: bool,
    pub checked: HashSet<i64>,
    pub selected: Vec<i64>,
    /// The row the keyboard and Shift+click start from.
    pub current: Option<i64>,
    pub visible: Vec<i64>,
    pub theme: ThemeName,
    pub palette: Palette,
    /// The theme changed: the window applies the style again.
    pub restyle: bool,
    pub view: View,
    pub boxx: Option<MsgBox>,
    /// The tip of the menu item or toolbar button under the mouse (QAction.setStatusTip).
    pub status_tip: String,
    pub details_tab: usize,
    /// Bring this row into view on the next frame.
    pub scroll_to: Option<i64>,
    pub quit: bool,
    last_tick: Option<Instant>,
}

fn column_width(key: &str, saved: i64) -> f32 {
    match key {
        "_check" => 34.0,
        "queue" => 30.0,
        _ => (saved.max(40)) as f32,
    }
}

impl App {
    pub fn new(rt: tokio::runtime::Runtime, manager: DownloadManager, dirs: Dirs) -> App {
        let theme = ThemeName::parse(&manager.settings.theme);
        let search = SearchSettings {
            query: manager.settings.search_string.clone(),
            flags: manager.settings.search_flags & 0x1F,
            active: false,
        };
        App {
            rt,
            manager,
            dirs,
            filter: TreeFilter::default(),
            finder: FindNext::new(search.clone()),
            search_text: search.query.clone(),
            search,
            search_open: false,
            focus_search: false,
            checked: HashSet::new(),
            selected: vec![],
            current: None,
            visible: vec![],
            theme,
            palette: Palette::of(theme),
            restyle: false,
            view: View::default(),
            boxx: None,
            status_tip: String::new(),
            details_tab: 0,
            scroll_to: None,
            quit: false,
            last_tick: None,
        }
    }

    /// Poll the manager when a tick is due; true when the view was rebuilt.
    pub fn tick(&mut self) -> bool {
        if self.last_tick.is_some_and(|t| t.elapsed() < TICK) {
            return false;
        }
        self.last_tick = Some(Instant::now());
        let _events = self.manager.poll(None);
        self.rebuild();
        true
    }

    pub fn save_settings(&self) {
        let _ = lider_core::settings::save(&self.manager.settings, &self.dirs);
    }

    /// The downloads an action applies to: the checked ones, else the selected ones.
    pub fn targets(&self) -> Vec<i64> {
        view::targets(&self.visible, &self.checked, &self.selected)
    }

    /// Recompute the view (after a tick or an action of the user).
    pub fn rebuild(&mut self) {
        let m = &self.manager;
        let tree = view::tree(m);
        self.filter = view::keep_filter(&tree, &self.filter);
        let visible = view::visible(m, &self.filter, &self.search);
        if visible != self.visible {
            view::prune_checks(&visible, &mut self.checked);
            let keep: HashSet<i64> = visible.iter().copied().collect();
            self.selected.retain(|i| keep.contains(i));
            if self.current.is_some_and(|c| !keep.contains(&c)) {
                self.current = None;
            }
            self.visible = visible;
        }
        let snaps: HashMap<i64, lider_core::engine::Snapshot> = m
            .downloads
            .keys()
            .filter(|i| m.is_running(**i))
            .filter_map(|i| m.task(*i).map(|t| (*i, t.snapshot())))
            .collect();
        let total: f64 = snaps.values().map(|s| s.rate).sum();
        let targets = view::targets(&self.visible, &self.checked, &self.selected);
        let checked_count = self
            .visible
            .iter()
            .filter(|i| self.checked.contains(i))
            .count();
        let actions = view::actions(m, &targets, checked_count);
        let details = self
            .current
            .or(self.selected.first().copied())
            .and_then(|id| m.downloads.get(&id))
            .map(|d| view::details(m, d, snaps.get(&d.id)));
        let widths: HashMap<&str, i64> = m
            .settings
            .columns
            .iter()
            .map(|c| (c.0.as_str(), c.2))
            .collect();
        self.view = View {
            // The check column first, then the user's visible columns.
            columns: std::iter::once("_check".to_string())
                .chain(view::columns(m).into_iter().filter(|k| k != "_check"))
                .map(|key| Column {
                    width: column_width(&key, widths.get(key.as_str()).copied().unwrap_or(110)),
                    key,
                })
                .collect(),
            rows: view::rows(m, &self.visible, &self.checked, &|id| {
                snaps.get(&id).map(|s| s.rate)
            }),
            status: Some(view::status_bar(m, total)),
            menus: lider_core::menu::menus(m, &actions),
            actions: Some(actions),
            details,
            queues: view::queue_names(m),
            tree,
        };
    }

    pub fn set_filter(&mut self, f: TreeFilter) {
        self.filter = f;
        self.rebuild();
    }

    pub fn info(&mut self, title: &str, text: &str) {
        self.boxx = Some(MsgBox::info(title, text));
    }

    /// A window of the dialogs phase that is not built yet.
    fn later(&mut self, what: &str) {
        self.info(
            "LID",
            &format!(
                "{what} comes with the dialogs of the egui window (phase E3 of docs/egui/PLAN.md)."
            ),
        );
    }

    fn set_theme(&mut self, name: &str) {
        self.theme = ThemeName::parse(name);
        self.palette = Palette::of(self.theme);
        self.restyle = true;
    }

    /// Every menu, toolbar and context-menu action, by its id (`name` or `name:argument`).
    pub fn run(&mut self, id: &str) {
        let (kind, arg) = id.split_once(':').unwrap_or((id, ""));
        match kind {
            "exit" => self.quit = true,
            "pause_all" | "stop_all" => self.manager.stop_all(),
            "stop" => {
                for i in self.targets() {
                    self.manager.stop(i);
                }
            }
            "resume" => {
                for i in self.targets() {
                    let start = self
                        .manager
                        .downloads
                        .get(&i)
                        .is_some_and(|d| d.status != Status::Complete)
                        && !self.manager.is_running(i);
                    if start {
                        self.manager.start(i);
                    }
                }
            }
            "redownload" => {
                for i in self.targets() {
                    self.manager.redownload(i, false, true);
                }
            }
            "start_queue" => {
                let _events = self.manager.start_queue(arg);
            }
            "stop_queue" => self.manager.stop_queue(arg),
            "limiter_on" | "limiter_off" => {
                self.manager.set_speed_limiter(kind == "limiter_on", None);
                self.save_settings();
            }
            "hide_categories" => {
                self.manager.settings.show_categories = !self.manager.settings.show_categories
            }
            "details" => self.manager.settings.show_details = !self.manager.settings.show_details,
            "statusbar" => {
                self.manager.settings.show_statusbar = !self.manager.settings.show_statusbar
            }
            "arrange" => {
                self.manager.settings.sort_key = arg.into();
                self.manager.settings.sort_desc = false;
            }
            "toolbar_labels" | "toolbar_icons" => {
                self.manager.settings.toolbar_labels = kind == "toolbar_labels"
            }
            "menubar" => {
                // The menu bar is always drawn in the window (decision G3); the setting keeps its value.
                self.manager.settings.menubar = if ["native", "custom", "both"].contains(&arg) {
                    arg
                } else {
                    "native"
                }
                .into();
            }
            "theme" => {
                if arg == "system" {
                    self.manager.settings.follow_system = true;
                } else {
                    self.manager.settings.follow_system = false;
                    self.manager.settings.theme = arg.into();
                    self.set_theme(arg);
                }
                self.save_settings();
            }
            "font_reset" => self.manager.settings.list_font.clear(),
            "remove" => {
                let ids = self.targets();
                if !ids.is_empty() {
                    let text = format!(
                        "Are you sure you want to delete {} file(s) from the download list?",
                        ids.len()
                    );
                    self.boxx = Some(
                        MsgBox::question("Confirm deletion of downloads", &text, Then::Remove(ids))
                            .with_icon("delete", true)
                            .with_checkbox("Also delete the downloaded files from disk"),
                    );
                }
            }
            "delete_completed" => {
                let n = self
                    .manager
                    .downloads
                    .values()
                    .filter(|d| d.status == Status::Complete)
                    .count();
                if n > 0 {
                    let text = format!(
                        "Remove all {n} completed file(s) from the list?\nThe files stay on the disk."
                    );
                    self.boxx = Some(MsgBox::question("LID", &text, Then::DeleteCompleted));
                }
            }
            "find" => {
                self.search_open = true;
                self.search_text = self.search.query.clone();
                self.focus_search = true;
            }
            "find_next" => self.find_next(),
            "about" => {
                let text = format!(
                    "Linux Internet Downloader {}\nLID · package lider\n\nA fast download manager for Linux: multi-connection downloads with dynamic segmentation, queues, scheduler, categories, YouTube, clipboard monitoring and search.\n\nMIT License.\n\nLID is an independent project, not affiliated with Tonec Inc. or Internet Download Manager.",
                    lider_core::VERSION
                );
                self.boxx = Some(MsgBox::info("About LID", &text).with_icon("app", false));
            }
            "export_ef2" | "export_text" => self.export(kind == "export_ef2"),
            "import_ef2" | "import_text" => self.import(kind == "import_ef2"),
            "batch_clipboard" => {
                let text = arboard::Clipboard::new()
                    .and_then(|mut c| c.get_text())
                    .unwrap_or_default();
                let urls: Vec<&str> = text
                    .split_whitespace()
                    .filter(|w| view::is_url(w))
                    .collect();
                if urls.is_empty() {
                    self.info("LID", "Cannot find any web address in the clipboard!");
                } else {
                    self.later("Download All Links");
                }
            }
            // Context menu of the list.
            "ctx_open" => self.first_target(|a, id| {
                if let Some(d) = a.manager.downloads.get(&id) {
                    let _ = os().open_path(&d.path());
                }
            }),
            "ctx_open_folder" => self.first_target(|a, id| a.open_folder(id)),
            "ctx_open_with" => {
                if let Some(id) = self.selected.first().copied() {
                    self.boxx = Some(MsgBox::input(
                        "Open with",
                        "Command (the file path is appended):",
                        Then::OpenWith(id),
                    ));
                }
            }
            "ctx_queue" => {
                let ids = self.selected.clone();
                self.manager.set_queue(&ids, arg);
            }
            "ctx_unqueue" => {
                let ids = self.selected.clone();
                self.manager.set_queue(&ids, "");
            }
            "ctx_copy" => self.first_target(|a, id| {
                if let Some(d) = a.manager.downloads.get(&id) {
                    let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(d.url.clone()));
                }
            }),
            // Context menu of the tree.
            "q_start" => {
                let _events = self.manager.start_queue(arg);
            }
            "q_stop" => self.manager.stop_queue(arg),
            "cat_delete" => {
                let text = format!(
                    "Do you really want to delete \"{arg}\" category from LID categories list?"
                );
                self.boxx = Some(MsgBox::question(
                    "Deleting a category from LID categories list",
                    &text,
                    Then::DeleteCategory(arg.into()),
                ));
            }
            // Windows of phase E3.
            "add" => self.later("Enter new address to download"),
            "batch" => self.later("Batch download"),
            "grabber" => self.later("The site grabber"),
            "scheduler" | "q_scheduler" => self.later("The Scheduler"),
            "limiter_settings" => self.later("Speed Limiter settings"),
            "options" | "appearance" => self.later("LID options"),
            "columns" => self.later("Columns"),
            "font_choose" => self.later("Choose Font"),
            "find_settings" => self.later("LID Search settings"),
            "ctx_properties" | "cat_add" | "cat_properties" => self.later("This window"),
            "ctx_check_hash" => self.later("Check hash"),
            _ => {}
        }
        self.rebuild();
    }

    fn first_target(&mut self, f: impl FnOnce(&mut App, i64)) {
        if let Some(id) = self.selected.first().copied() {
            f(self, id);
        }
    }

    /// Show the file in the file manager when it exists, its folder otherwise.
    pub fn open_folder(&mut self, id: i64) {
        let Some(d) = self.manager.downloads.get(&id) else {
            return;
        };
        let _ = if d.status == Status::Complete {
            os().open_folder(&d.path())
        } else {
            os().open_path(&d.save_dir)
        };
    }

    /// Double-click: a finished download opens Properties, any other one starts.
    pub fn activate(&mut self, id: i64) {
        let complete = self
            .manager
            .downloads
            .get(&id)
            .is_some_and(|d| d.status == Status::Complete);
        if complete {
            self.later("File Properties");
        } else if !self.manager.is_running(id) {
            self.manager.start(id);
        }
        self.rebuild();
    }

    /// The details pane's first button: Pause, Skip check, Download again, Open or Resume.
    pub fn details_button(&mut self, id: i64, action: &str) {
        match action {
            "pause" => self.manager.stop(id),
            "skip" => self.manager.skip_check(id),
            "again" => self.manager.redownload(id, true, true),
            "open" => {
                if let Some(d) = self.manager.downloads.get(&id) {
                    let _ = os().open_path(&d.path());
                }
            }
            _ => {
                self.manager.start(id);
            }
        }
        self.rebuild();
    }

    /// The download is finished, being checked, has data on disk or runs: its hash can be checked.
    pub fn can_check(&self, id: i64) -> bool {
        self.manager.downloads.get(&id).is_some_and(|d| {
            matches!(d.status, Status::Complete | Status::Verifying)
                || d.downloaded > 0
                || self.manager.is_running(id)
        })
    }

    // ------------------------------------------------------------------ list

    pub fn header_click(&mut self, key: &str) {
        if !view::SORT_KEYS.contains(&key) {
            return;
        }
        let s = &mut self.manager.settings;
        if s.sort_key == key {
            s.sort_desc = !s.sort_desc;
        } else {
            s.sort_key = key.into();
            s.sort_desc = false;
        }
        self.rebuild();
    }

    pub fn toggle_all(&mut self) {
        view::toggle_all(&self.visible, &mut self.checked);
        self.rebuild();
    }

    pub fn check(&mut self, id: i64, on: bool) {
        if on {
            self.checked.insert(id);
        } else {
            self.checked.remove(&id);
        }
        self.rebuild();
    }

    /// A click on a row: plain selects it, Ctrl toggles it, Shift selects from the current row to it.
    pub fn click_row(&mut self, id: i64, ctrl: bool, shift: bool) {
        if shift && let Some(from) = self.current {
            let pos = |x: i64| self.visible.iter().position(|v| *v == x);
            if let (Some(a), Some(b)) = (pos(from), pos(id)) {
                let range = self.visible[a.min(b)..=a.max(b)].to_vec();
                self.selected = if ctrl {
                    self.selected
                        .iter()
                        .copied()
                        .chain(range)
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect()
                } else {
                    range
                };
                self.selected.sort_by_key(|x| pos(*x));
                self.rebuild();
                return;
            }
        }
        if ctrl {
            if let Some(i) = self.selected.iter().position(|x| *x == id) {
                self.selected.remove(i);
            } else {
                self.selected.push(id);
            }
        } else {
            self.selected = vec![id];
        }
        self.current = Some(id);
        self.rebuild();
    }

    /// Up, Down, Home, End: move the current row by `delta` rows (or to an end).
    pub fn move_current(&mut self, delta: i64, to_end: Option<bool>) {
        if self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() as i64 - 1;
        let at = self
            .current
            .and_then(|c| self.visible.iter().position(|v| *v == c))
            .map(|i| i as i64);
        let next = match (to_end, at) {
            (Some(false), _) => 0,
            (Some(true), _) => last,
            (None, Some(i)) => (i + delta).clamp(0, last),
            (None, None) => 0,
        };
        let id = self.visible[next as usize];
        self.selected = vec![id];
        self.current = Some(id);
        self.scroll_to = Some(id);
        self.rebuild();
    }

    pub fn select_all(&mut self) {
        self.selected = self.visible.clone();
        self.rebuild();
    }

    // ------------------------------------------------------------------ search

    fn persist_search(&mut self) {
        self.manager.settings.search_string = self.search.query.clone();
        self.manager.settings.search_flags = self.search.flags;
        self.save_settings();
    }

    /// The search bar's Find: the list shows the matches only.
    pub fn run_search(&mut self) {
        self.search.query = self.search_text.clone();
        self.search.active = !self.search.query.is_empty();
        let checked = self.search.validate().map_err(|e| e.to_string());
        self.finder = FindNext::new(self.search.clone());
        self.persist_search();
        if let Err(e) = checked {
            self.info("LID Search settings", &e);
        }
        self.rebuild();
    }

    pub fn close_search(&mut self) {
        self.search_open = false;
        self.search.active = false;
        self.rebuild();
    }

    /// F3: select the next match in the list as it is shown.
    pub fn find_next(&mut self) {
        if self.search.query.is_empty() {
            self.later("LID Search settings");
            return;
        }
        let rows: Vec<lider_core::models::Download> = self
            .visible
            .iter()
            .filter_map(|i| self.manager.downloads.get(i).cloned())
            .collect();
        self.finder.settings.query = self.search.query.clone();
        self.finder.settings.flags = self.search.flags;
        match self.finder.next(&rows) {
            Ok(Some(i)) => {
                let id = rows[i].id;
                self.selected = vec![id];
                self.current = Some(id);
                self.scroll_to = Some(id);
            }
            Ok(None) => self.info("Find file", "No such files found"),
            Err(e) => self.info("LID Search settings", &e.to_string()),
        }
        self.persist_search();
        self.rebuild();
    }

    // ------------------------------------------------------------------ export, import, boxes

    fn export(&mut self, ef2: bool) {
        let dialog = rfd::FileDialog::new().set_title("Export download list");
        let dialog = if ef2 {
            dialog
                .set_file_name("downloads.ef2")
                .add_filter("Export files", &["ef2"])
        } else {
            dialog
                .set_file_name("downloads.txt")
                .add_filter("Text files", &["txt"])
        };
        if let Some(path) = dialog.save_file() {
            let m = &self.manager;
            let text = if ef2 {
                lider_core::ef2::export_ef2(m.downloads.values())
            } else {
                lider_core::ef2::export_text(m.downloads.values())
            };
            if let Err(e) = std::fs::write(path, text) {
                self.info("LID", &e.to_string());
            }
        }
    }

    fn import(&mut self, ef2: bool) {
        let dialog = rfd::FileDialog::new().set_title("Import");
        let dialog = if ef2 {
            dialog.add_filter("Export files", &["ef2"])
        } else {
            dialog.add_filter("Text files", &["txt"])
        };
        let Some(path) = dialog.add_filter("All files", &["*"]).pick_file() else {
            return;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let text = String::from_utf8_lossy(&bytes);
        let items: Vec<(String, String, String)> = if ef2 {
            lider_core::ef2::parse_ef2(&text)
                .into_iter()
                .map(|(u, r)| (u, r, String::new()))
                .collect()
        } else {
            lider_core::ef2::urls_in(&text)
                .into_iter()
                .map(|u| (u, String::new(), String::new()))
                .collect()
        };
        self.add_many(&items);
    }

    /// Batch, drop and import: add to the Main queue, then ask whether to start it.
    pub fn add_many(&mut self, items: &[(String, String, String)]) {
        let n = view::add_many(&mut self.manager, items, "", "");
        if n > 0 {
            let text =
                format!("{n} file(s) were added to the \"{MAIN_QUEUE}\". Start the queue now?");
            self.boxx = Some(MsgBox::question("LID", &text, Then::StartMainQueue));
        }
        self.rebuild();
    }

    /// A message box was answered.
    pub fn answer(&mut self, a: Answer) {
        let yes = a.button == "yes" || a.button == "ok";
        match a.then {
            Then::Nothing => {}
            Then::Remove(ids) if yes => {
                self.manager.remove(&ids, a.checked);
                for i in &ids {
                    self.checked.remove(i);
                }
            }
            Then::DeleteCompleted if yes => {
                self.manager.remove_completed();
            }
            Then::StartMainQueue if yes => {
                let _events = self.manager.start_queue(MAIN_QUEUE);
            }
            Then::DeleteCategory(name) if yes => {
                self.manager.settings.categories.retain(|c| c.name != name);
                let ids: Vec<i64> = self
                    .manager
                    .downloads
                    .values()
                    .filter(|d| d.category == name)
                    .map(|d| d.id)
                    .collect();
                for id in ids {
                    self.manager.edit(id, |d| d.category = "General".into());
                }
                self.save_settings();
                self.filter = TreeFilter::default();
            }
            Then::OpenWith(id) if yes && !a.value.trim().is_empty() => {
                if let Some(d) = self.manager.downloads.get(&id)
                    && let Err(e) = os().open_with(&a.value, &d.path())
                {
                    self.info("Open with", &e.to_string());
                }
            }
            _ => {}
        }
        self.rebuild();
    }
}
