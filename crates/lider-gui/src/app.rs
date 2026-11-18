//! The running program: the download manager, what the user has selected, the 500 ms tick that polls the
//! manager and rebuilds what the window shows (the view model of `lider_core::view`), and every action of
//! the menus, the toolbar and the context menus (`main_window.py` in 1.x). Also what LID does while it
//! runs (phase E2): the manager's events, lines from other starts and the browser, the clipboard monitor,
//! closing and the tray, the power-off countdown. What touches the desktop leaves as an [`Effect`] that
//! the window carries out, so tests see it without side effects.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use lider_core::manager::{DownloadManager, Event, EventKind};
use lider_core::menu::Menu;
use lider_core::models::{MAIN_QUEUE, Status};
use lider_core::paths::Dirs;
use lider_core::search::{FindNext, SearchSettings};
use lider_core::settings::WindowState;
use lider_core::view::{self, ActionState, Details, Row, StatusBar, TreeFilter, TreeNode};
use lider_sys::platform::prelude::*;
use serde_json::{Map, Value};

use crate::boxes::{Answer, Button, MsgBox, Then};
use crate::dialogs::{self, Dialog};
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

/// Something only the desktop can do; the window carries it out.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A desktop notification: title, text.
    Notify(String, String),
    /// Show the window (unminimise, raise).
    Show,
    /// Hide the window (to the tray).
    Hide,
    /// Close the window for good (the manager is already stopped).
    Close,
    /// Turn the computer off (the countdown ended).
    PowerOff,
}

/// A dialog window that the manager, another start or the browser asks for (opened on the next tick).
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Add one address (the add flow: probe, duplicate rule, Download File Info).
    AddUrl {
        url: String,
        referer: String,
        parent_page: String,
        headers: Map<String, Value>,
        filename: String,
    },
    /// Links to choose from (browser "all links", grabber, clipboard batch).
    Links {
        items: Vec<(String, String, String)>,
        referer: String,
        parent_page: String,
        headers: Map<String, Value>,
    },
    /// "Checksum mismatch": what to do with the file.
    Mismatch(i64),
}

impl Request {
    pub fn add(url: &str) -> Request {
        Request::AddUrl {
            url: url.into(),
            referer: String::new(),
            parent_page: String::new(),
            headers: Map::new(),
            filename: String::new(),
        }
    }
}

/// The power-off countdown, seconds.
pub const POWER_OFF_SECONDS: u64 = 30;

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
    /// What the window should do on the desktop (drained each frame).
    pub effects: Vec<Effect>,
    /// Dialog windows asked for; opened on the next tick.
    pub requests: Vec<Request>,
    /// The open dialog windows (crate::dialogs).
    pub dialogs: Vec<Dialog>,
    /// A font chosen in Choose Font for the Options window (it takes it on its next frame).
    pub font_chosen: Option<String>,
    /// Lines from the instance socket (other starts, the browser helper), taken each tick.
    pub inbox: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// The clipboard text of the last tick: None until the first read, so only a change counts.
    clip_seen: Option<String>,
    last_clip: String,
    /// The desktop prefers a dark theme (for "follow the system").
    pub dark_system: bool,
    /// The tray icon is shown (the window keeps it; closing may hide to it).
    pub tray_shown: bool,
    /// Exit was chosen (Tasks → Exit or the tray), not the window's close button.
    really_quit: bool,
    /// The manager is stopped and the window closes; a second close request just goes through.
    closing: bool,
    /// When the power-off countdown ends.
    pub power_off_at: Option<Instant>,
    /// The window's place and size (kept while it is shown, saved with the settings).
    pub geometry: Option<WindowState>,
    /// "Browser: connected" (any detected browser has the helper); None = check again (Options and
    /// Browser integration reset it).
    pub browser_ok: Option<bool>,
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
        let theme = ThemeName::parse(&view::resolve_theme(&manager.settings, false));
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
            effects: vec![],
            requests: vec![],
            dialogs: vec![],
            font_chosen: None,
            inbox: Default::default(),
            clip_seen: None,
            last_clip: String::new(),
            dark_system: false,
            tray_shown: false,
            really_quit: false,
            closing: false,
            power_off_at: None,
            geometry: None,
            browser_ok: None,
        }
    }

    /// Poll the manager when a tick is due; true when the view was rebuilt.
    pub fn tick(&mut self) -> bool {
        if self.last_tick.is_some_and(|t| t.elapsed() < TICK) {
            return false;
        }
        self.last_tick = Some(Instant::now());
        let lines = std::mem::take(&mut *self.inbox.lock().unwrap_or_else(|e| e.into_inner()));
        for l in lines {
            self.on_line(&l);
        }
        let events = self.manager.poll(None);
        self.handle_events(&events);
        self.countdown();
        self.take_requests();
        self.rebuild();
        true
    }

    /// A tick now, whenever the last one was (tests).
    #[cfg(test)]
    pub fn tick_now(&mut self) {
        self.last_tick = None;
        self.tick();
    }

    pub fn save_settings(&mut self) {
        if let Some(g) = self.geometry {
            self.manager.settings.window_state = Some(g);
        }
        let _ = lider_core::settings::save(&self.manager.settings, &self.dirs);
    }

    fn notify(&mut self, title: &str, text: &str) {
        self.effects
            .push(Effect::Notify(title.to_string(), text.to_string()));
    }

    fn file_of(&self, id: i64) -> String {
        self.manager
            .downloads
            .get(&id)
            .map(|d| d.filename.clone())
            .unwrap_or_default()
    }

    /// The theme that applies now: the chosen one, or the system's light or dark one.
    pub fn apply_theme(&mut self) {
        let name = view::resolve_theme(&self.manager.settings, self.dark_system);
        let t = ThemeName::parse(&name);
        if t != self.theme {
            self.set_theme(&name);
        }
    }

    // ------------------------------------------------------------------ the manager's events

    /// `handle_events` of 1.x: what the window does with the manager's events.
    pub fn handle_events(&mut self, events: &[Event]) {
        for e in events {
            let id = e.download_id.unwrap_or(0);
            match e.kind {
                EventKind::Complete => self.download_complete(id),
                EventKind::Error => {
                    // The progress window shows the error itself.
                    if !self.dialog_open(&dialogs::progress::label(id)) {
                        let text = format!("{}\n{}", self.file_of(id), e.message);
                        self.notify("Download failed", &text);
                    }
                }
                EventKind::Mismatch => {
                    self.close_dialog(&dialogs::progress::label(id));
                    if e.message == "kept" {
                        let text = format!("{}\nThe file was kept.", self.file_of(id));
                        self.notify("Checksum mismatch", &text);
                    } else {
                        self.requests.push(Request::Mismatch(id));
                    }
                }
                EventKind::Changed if !e.message.is_empty() => {
                    let text = format!("{}\nDownloading it again.", self.file_of(id));
                    self.notify("Checksum mismatch", &text);
                }
                EventKind::QueueDone => {
                    let name = e.queue.clone().unwrap_or_default();
                    let Some(s) = self.manager.queues.get(&name).map(|q| q.schedule.clone()) else {
                        continue;
                    };
                    let how = if e.message.is_empty() {
                        "finished"
                    } else {
                        "stopped"
                    };
                    self.notify("LID", &format!("\"{name}\" processing {how}"));
                    if !s.open_when_done.is_empty() {
                        let _ = os().open_path(&s.open_when_done);
                    }
                    if s.shutdown_when_done {
                        self.start_power_off();
                    } else if s.exit_when_done {
                        self.exit();
                    }
                }
                _ => {}
            }
        }
    }

    /// A download finished: its progress window closes; "Download complete" (or a notification), then
    /// what the progress window's "on completion" options ask for.
    fn download_complete(&mut self, id: i64) {
        let mut opts = dialogs::progress::completion_options(self, id).unwrap_or(
            dialogs::progress::CompletionOptions {
                show_complete: self.manager.settings.show_complete_dialog,
                exit: false,
                shutdown: false,
            },
        );
        self.close_dialog(&dialogs::progress::label(id));
        let in_running_queue = self
            .manager
            .downloads
            .get(&id)
            .and_then(|d| self.manager.queues.get(&d.queue))
            .is_some_and(|q| q.running);
        if in_running_queue {
            opts.show_complete = false;
        }
        if opts.show_complete {
            dialogs::complete::complete(self, id);
        } else {
            let name = self.file_of(id);
            self.notify("Download complete", &name);
        }
        if opts.shutdown {
            self.start_power_off();
        } else if opts.exit {
            self.exit();
        }
    }

    // ------------------------------------------------------------------ the power-off countdown

    /// The 30 s countdown before the computer is turned off (a scheduler queue or a progress window).
    pub fn start_power_off(&mut self) {
        self.effects.push(Effect::Show);
        self.power_off_at = Some(Instant::now() + Duration::from_secs(POWER_OFF_SECONDS));
        self.boxx = Some(
            MsgBox::info("Shut down", &power_off_text(POWER_OFF_SECONDS))
                .with_icon("info", true)
                .with_buttons(vec![Button {
                    id: "cancel",
                    label: "Cancel",
                    primary: true,
                    danger: false,
                }]),
        );
        if let Some(b) = &mut self.boxx {
            b.then = Then::PowerOff;
        }
    }

    fn countdown(&mut self) {
        let Some(at) = self.power_off_at else { return };
        let left = at.saturating_duration_since(Instant::now());
        if left.is_zero() {
            self.power_off_at = None;
            self.boxx = None;
            self.save_settings();
            self.manager.shutdown();
            self.effects.push(Effect::PowerOff);
            return;
        }
        if let Some(b) = self.boxx.as_mut().filter(|b| b.then == Then::PowerOff) {
            b.text = power_off_text(left.as_secs_f64().ceil() as u64);
        }
    }

    // ------------------------------------------------------------------ other starts, the browser, the clipboard

    /// A line from another start or from the browser helper.
    pub fn on_line(&mut self, line: &str) {
        if line.starts_with('{') {
            if let Ok(Value::Object(req)) = serde_json::from_str::<Value>(line) {
                self.browser_request(&req);
            }
            return;
        }
        self.effects.push(Effect::Show);
        if line != "--show" {
            self.requests.push(Request::add(line));
        }
    }

    /// `handle_browser_request` of 1.x.
    pub fn browser_request(&mut self, req: &Map<String, Value>) {
        let get = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        if get("action") == "show" {
            self.effects.push(Effect::Show);
            return;
        }
        let mut headers = Map::new();
        if !get("cookies").is_empty() {
            headers.insert("Cookie".into(), get("cookies").into());
        }
        if !get("user_agent").is_empty() {
            headers.insert("User-Agent".into(), get("user_agent").into());
        }
        let page = get("page_url");
        let referer = if get("referer").is_empty() {
            page.clone()
        } else {
            get("referer")
        };
        match get("action").as_str() {
            "download" => {
                let url = get("url");
                if view::is_url(&url) {
                    self.requests.push(Request::AddUrl {
                        url,
                        referer,
                        parent_page: page,
                        headers,
                        filename: get("filename"),
                    });
                }
            }
            "links" => {
                let items: Vec<(String, String, String)> = req
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|i| {
                        let url = i.get("url")?.as_str()?;
                        let text = |k: &str, d: &str| {
                            i.get(k).and_then(Value::as_str).unwrap_or(d).to_string()
                        };
                        view::is_url(url)
                            .then(|| (url.to_string(), text("text", ""), text("kind", "a")))
                    })
                    .collect();
                if !items.is_empty() {
                    self.effects.push(Effect::Show);
                    self.requests.push(Request::Links {
                        items,
                        referer,
                        parent_page: page,
                        headers,
                    });
                }
            }
            _ => {}
        }
    }

    /// The clipboard monitor (1.x `_clipboard_changed`), called with the clipboard's text each tick.
    pub fn clipboard_text(&mut self, text: &str) {
        let text = text.trim();
        // Polling stands in for a change signal: what is there at startup, or while the monitor is off,
        // is not a change.
        let first = self.clip_seen.is_none();
        if self.clip_seen.as_deref() == Some(text) {
            return;
        }
        self.clip_seen = Some(text.to_string());
        if first || !self.manager.settings.monitor_clipboard {
            return;
        }
        if text == self.last_clip || !view::is_url(text) {
            return;
        }
        self.last_clip = text.to_string();
        let s = &self.manager.settings;
        let host = lider_core::pyutil::url_hostname(text).unwrap_or_default();
        if s.no_auto_sites
            .split_whitespace()
            .any(|site| host.ends_with(&site.to_lowercase()))
        {
            return;
        }
        let name = lider_core::names::filename_from_url(text);
        if lider_core::media::is_youtube(text)
            || lider_core::categories::is_auto_capture_type(s, &name)
        {
            self.requests.push(Request::add(text));
        }
    }

    /// Ctrl+V on the window (decision G2, in place of dropping a link): one address opens the add flow,
    /// several go to the Main queue.
    pub fn paste(&mut self, text: &str) {
        let urls: Vec<String> = text
            .split_whitespace()
            .filter(|w| !w.starts_with('#') && view::is_url(w))
            .map(String::from)
            .collect();
        match urls.len() {
            0 => {}
            1 => self.requests.push(Request::add(&urls[0])),
            _ => {
                let items: Vec<_> = urls
                    .into_iter()
                    .map(|u| (u, String::new(), String::new()))
                    .collect();
                self.add_many(&items);
            }
        }
    }

    /// Open the windows asked for.
    pub fn take_requests(&mut self) {
        for r in std::mem::take(&mut self.requests) {
            match r {
                r @ Request::AddUrl { .. } => dialogs::add::from_request(self, r),
                Request::Links {
                    items,
                    referer,
                    parent_page,
                    headers,
                } => dialogs::links::open(self, items, referer, parent_page, headers),
                Request::Mismatch(id) => dialogs::complete::mismatch(self, id),
            }
        }
    }

    // ------------------------------------------------------------------ closing

    /// The downloads that are running or being checked, as the closing question lists them.
    fn busy(&self) -> Vec<(String, String, String)> {
        let m = &self.manager;
        let mut v: Vec<_> = m
            .downloads
            .values()
            .filter(|d| m.is_running(d.id) || d.status == Status::Verifying)
            .map(|d| {
                let done = if d.size > 0 {
                    format!("{:.0}%", d.progress() * 100.0)
                } else {
                    lider_core::fmt::size(d.downloaded)
                };
                (view::category_icon(&d.category), d.filename.clone(), done)
            })
            .collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v
    }

    /// The close button or Exit. True when the window may close now.
    pub fn close_requested(&mut self) -> bool {
        if self.closing {
            return true;
        }
        let really = self.really_quit;
        let s = &self.manager.settings;
        if s.tray_icon && self.tray_shown && s.close_to_tray && !really {
            self.effects.push(Effect::Hide);
            return false;
        }
        let running = self.busy();
        if running.is_empty() {
            self.stop_and_exit();
            return true;
        }
        self.really_quit = false;
        self.effects.push(Effect::Show);
        let background = !(really && self.tray_shown);
        let n = running.len();
        let what = if really {
            "Exiting"
        } else {
            "Closing the window"
        };
        let them = if n == 1 { "it" } else { "them" };
        let title = if n == 1 {
            "1 download is still running".to_string()
        } else {
            format!("{n} downloads are still running")
        };
        let text = if background {
            format!(
                "{what} stops {them}. LID can keep downloading in the background, with an icon in the system tray."
            )
        } else {
            format!("{what} stops {them}. Unfinished downloads can be resumed later.")
        };
        let mut buttons = vec![Button {
            id: "stop",
            label: if really {
                "Stop and exit"
            } else {
                "Stop and close"
            },
            primary: false,
            danger: true,
        }];
        if background {
            buttons.push(Button {
                id: "background",
                label: "Continue in background",
                primary: true,
                danger: false,
            });
        }
        buttons.push(Button {
            id: "keep",
            label: "Keep downloading",
            primary: false,
            danger: false,
        });
        let mut b = MsgBox::question(&title, &text, Then::CloseChoice)
            .with_icon("queue", false)
            .with_buttons(buttons);
        b.more = (n > 3).then(|| format!("and {} more", n - 3));
        b.items = running.into_iter().take(3).collect();
        self.boxx = Some(b);
        false
    }

    /// "Exit": closing for real, also when the window would go to the tray.
    pub fn exit(&mut self) {
        self.really_quit = true;
        if self.close_requested() {
            self.quit = true;
        }
    }

    pub fn stop_and_exit(&mut self) {
        self.closing = true;
        self.close_all_dialogs();
        self.save_settings();
        self.manager.shutdown();
        self.effects.push(Effect::Close);
        self.quit = true;
    }

    /// "Continue in background": the tray takes over.
    fn keep_in_background(&mut self) {
        self.manager.settings.tray_icon = true;
        self.manager.settings.close_to_tray = true;
        self.save_settings();
        self.effects.push(Effect::Hide);
        self.notify(
            "LID keeps downloading",
            "Open LID from this icon. Turn the icon off again in Options → Appearance.",
        );
    }

    /// Ask once whether to add LID to the application menu (a copy no package installed).
    pub fn offer_menu(&mut self, name: &str) {
        self.manager.settings.menu_offered = true;
        self.save_settings();
        self.boxx = Some(
            MsgBox::question(
                "Add LID to the application menu?",
                &format!("{name} runs on its own, so the application menu does not list LID yet."),
                Then::OfferMenu,
            )
            .with_icon("general", false)
            .with_note("LID keeps the entry pointed at the AppImage you start, also after you move or update it, and it disappears when the AppImage is deleted. You can change this later in Options → General.")
            .with_buttons(vec![
                Button {
                    id: "later",
                    label: "Not now",
                    primary: false,
                    danger: false,
                },
                Button {
                    id: "add",
                    label: "Add to menu",
                    primary: true,
                    danger: false,
                },
            ]),
        );
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

    /// A window that is not built yet.
    #[allow(dead_code)]
    pub fn later(&mut self, what: &str) {
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
            "exit" => self.exit(),
            "pause_all" | "stop_all" => self.manager.stop_all(),
            "stop" => {
                for i in self.targets() {
                    self.manager.stop(i);
                }
            }
            "resume" => {
                let ids = self.targets();
                let one = ids.len() == 1 && !self.manager.settings.show_details;
                for i in ids {
                    let start = self
                        .manager
                        .downloads
                        .get(&i)
                        .is_some_and(|d| d.status != Status::Complete)
                        && !self.manager.is_running(i);
                    if start {
                        self.manager.start(i);
                        if one {
                            dialogs::progress::open(self, i, false);
                        }
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
                    self.apply_theme();
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
                    let items = urls
                        .iter()
                        .map(|u| (u.to_string(), String::new(), "a".to_string()))
                        .collect();
                    dialogs::links::open(self, items, String::new(), String::new(), Map::new());
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
            // Dialog windows.
            "add" => dialogs::add::add_url(self),
            "batch" => dialogs::add::batch(self),
            "grabber" => dialogs::links::grabber(self),
            "scheduler" => dialogs::scheduler::open(self, MAIN_QUEUE),
            "q_scheduler" => {
                let q = if arg.is_empty() { MAIN_QUEUE } else { arg };
                dialogs::scheduler::open(self, q)
            }
            "limiter_settings" => dialogs::small::limiter(self),
            "options" => dialogs::options::open(self, ""),
            "appearance" => dialogs::options::open(self, "Appearance"),
            "browsers" => dialogs::browsers::open(self),
            "columns" => dialogs::small::columns(self),
            "font_choose" => dialogs::small::font(self, None),
            "find_settings" => dialogs::small::find(self, false),
            "cat_add" => dialogs::small::category(self, None),
            "cat_properties" => dialogs::small::category(self, Some(arg)),
            "ctx_properties" => self.first_target(dialogs::properties::open),
            "ctx_check_hash" => self.first_target(dialogs::checkhash::open),
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
            dialogs::properties::open(self, id);
        } else {
            if !self.manager.is_running(id) {
                self.manager.start(id);
            }
            dialogs::progress::open(self, id, false);
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

    pub fn save_search(&mut self) {
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
        self.save_search();
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
            dialogs::small::find(self, true);
            return;
        }
        match self.find_next_match() {
            Ok(Some(_)) => {}
            Ok(None) => self.info("Find file", "No such files found"),
            Err(e) => self.info("LID Search settings", &e),
        }
        self.rebuild();
    }

    /// Select the next match in the list as it is shown; its id, None when nothing matches.
    pub fn find_next_match(&mut self) -> Result<Option<i64>, String> {
        let rows: Vec<lider_core::models::Download> = self
            .visible
            .iter()
            .filter_map(|i| self.manager.downloads.get(i).cloned())
            .collect();
        self.finder.settings.query = self.search.query.clone();
        self.finder.settings.flags = self.search.flags;
        let found = self.finder.next(&rows).map_err(|e| e.to_string());
        let id = found.as_ref().ok().copied().flatten().map(|i| rows[i].id);
        if let Some(id) = id {
            self.selected = vec![id];
            self.current = Some(id);
            self.scroll_to = Some(id);
        }
        self.save_search();
        found.map(|_| id)
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
            Then::CloseChoice => match a.button {
                "stop" => self.stop_and_exit(),
                "background" => self.keep_in_background(),
                _ => {}
            },
            Then::PowerOff => self.power_off_at = None,
            Then::OfferMenu if a.button == "add" => {
                if let Err(e) = os().add_to_menu() {
                    self.info("LID", &e.to_string());
                }
            }
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

fn power_off_text(left: u64) -> String {
    format!("LID has finished downloading. The computer will be turned off in {left} seconds.")
}
