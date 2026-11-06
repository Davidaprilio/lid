//! The window's state and the main-window logic of `main_window.py` (1.x) that does not draw anything:
//! what the window shows each tick, the reactions to the manager's events, browser requests, the
//! clipboard monitor and closing.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};

use lider_core::manager::{DownloadManager, Event, EventKind};
use lider_core::models::{Download, Status};
use lider_core::paths::Dirs;
use lider_core::search::{FindNext, SearchSettings};
use lider_core::settings::{Settings, WindowState};
use lider_core::view::{
    self, ActionState, CheckState, Details, Row, StatusBar, TreeFilter, TreeNode,
};
use lider_sys::platform::prelude::*;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::menu::{self, Menu};

/// What the window keeps between ticks.
pub struct Ui {
    pub filter: TreeFilter,
    pub search: SearchSettings,
    pub finder: FindNext,
    pub checked: HashSet<i64>,
    pub selected: Vec<i64>,
    pub current: Option<i64>,
    /// The ids the list shows, in order (from the last tick).
    pub visible: Vec<i64>,
    /// "Browser: connected" (any detected browser has the host); None = check again.
    pub browser_ok: Option<bool>,
    pub last_clip: String,
    /// The clipboard text of the last tick: `None` until the first read, so only a change counts.
    pub clip_seen: Option<String>,
    /// Exit was chosen (Tasks → Exit or the tray), not the window's close button.
    pub really_quit: bool,
    pub dark_system: bool,
    pub global_menu: bool,
    pub tick: u64,
    pub menus_json: String,
    /// The theme dialog windows were last told about.
    pub last_theme: String,
    /// Addresses typed in "Enter new address to download" (most recent first, at most 20, not saved).
    pub url_history: Vec<String>,
}

pub struct AppState {
    pub manager: Mutex<DownloadManager>,
    pub ui: Mutex<Ui>,
    /// Lines from other starts and the browser, until the window is ready.
    pub pending: Mutex<Vec<String>>,
    pub ready: Mutex<bool>,
    pub offer_menu: Mutex<bool>,
    pub start_hidden: bool,
    pub dirs: Dirs,
    /// The OS adapter (lider_sys::platform): every OS service goes through its ports.
    pub sys: &'static lider_sys::platform::Os,
    /// Kept alive for the lifetime of the app: the download engine and the instance socket.
    pub _engine: tokio::runtime::Runtime,
    pub _server: Mutex<Option<lider_sys::platform::InstanceGuard>>,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn manager(&self) -> MutexGuard<'_, DownloadManager> {
        lock(&self.manager)
    }
    pub fn ui(&self) -> MutexGuard<'_, Ui> {
        lock(&self.ui)
    }
}

pub fn new_ui(settings: &Settings) -> Ui {
    let search = SearchSettings {
        query: settings.search_string.clone(),
        flags: settings.search_flags & 0x1F,
        active: false,
    };
    Ui {
        filter: TreeFilter::default(),
        finder: FindNext::new(search.clone()),
        search,
        checked: HashSet::new(),
        selected: vec![],
        current: None,
        visible: vec![],
        browser_ok: None,
        last_clip: String::new(),
        clip_seen: None,
        really_quit: false,
        dark_system: false,
        global_menu: false,
        tick: 0,
        menus_json: String::new(),
        last_theme: String::new(),
        url_history: Vec::new(),
    }
}

// ------------------------------------------------------------------ the view of a tick

#[derive(Serialize)]
pub struct Column {
    pub key: String,
    pub title: String,
    pub width: i64,
    pub min: i64,
    pub mono: bool,
}

#[derive(Serialize)]
pub struct Appearance {
    pub theme: String,
    pub density: String,
    pub toolbar_labels: bool,
    pub show_categories: bool,
    pub show_details: bool,
    pub show_statusbar: bool,
    /// Draw the menu bar in the window (custom, or both with a global menu).
    pub inline_menubar: bool,
    pub list_font: Option<(String, f64)>,
    pub sidebar_width: i64,
    pub details_height: i64,
    pub sort_key: String,
    pub sort_desc: bool,
}

#[derive(Serialize)]
pub struct MainView {
    pub title: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub check_state: CheckState,
    pub tree: Vec<TreeNode>,
    pub filter: TreeFilter,
    pub actions: ActionState,
    pub status: StatusBar,
    pub browser: String,
    pub details: Option<Details>,
    pub appearance: Appearance,
    pub search: SearchView,
    pub selected: Vec<i64>,
    pub current: Option<i64>,
    pub queues: Vec<String>,
    /// Sent only when it changed since the last tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menus: Option<Vec<Menu>>,
}

#[derive(Serialize)]
pub struct SearchView {
    pub query: String,
    pub flags: i64,
    pub active: bool,
}

/// Which theme applies now (`theme.resolve` in 1.x).
pub fn resolve_theme(s: &Settings, dark_system: bool) -> String {
    let valid = |t: &str| ["ember", "ember-dark", "dark"].contains(&t);
    if s.follow_system {
        if dark_system {
            if valid(&s.dark_theme) {
                s.dark_theme.clone()
            } else {
                "ember-dark".into()
            }
        } else {
            "ember".into()
        }
    } else if valid(&s.theme) {
        s.theme.clone()
    } else {
        "ember".into()
    }
}

pub fn inline_menubar(s: &Settings, global_menu: bool) -> bool {
    match s.menubar.as_str() {
        "custom" => true,
        "both" => global_menu,
        _ => false,
    }
}

/// Everything the main window draws. `force_menus` sends the menus even when unchanged.
pub fn main_view(state: &AppState, force_menus: bool) -> MainView {
    let m = state.manager();
    let mut ui = state.ui();
    let nodes = view::tree(&m);
    ui.filter = view::keep_filter(&nodes, &ui.filter);
    let visible = view::visible(&m, &ui.filter, &ui.search);
    if visible != ui.visible {
        let mut checked = std::mem::take(&mut ui.checked);
        view::prune_checks(&visible, &mut checked);
        ui.checked = checked;
        let keep: HashSet<i64> = visible.iter().copied().collect();
        ui.selected.retain(|i| keep.contains(i));
        if ui.current.is_some_and(|c| !keep.contains(&c)) {
            ui.current = None;
        }
        ui.visible = visible.clone();
    }
    let snaps: std::collections::HashMap<i64, lider_core::engine::Snapshot> = visible
        .iter()
        .filter(|i| m.is_running(**i))
        .filter_map(|i| m.task(*i).map(|t| (*i, t.snapshot())))
        .collect();
    let rows = view::rows(&m, &visible, &ui.checked, &|id| {
        snaps.get(&id).map(|s| s.rate)
    });
    let targets = view::targets(&visible, &ui.checked, &ui.selected);
    let checked_count = visible.iter().filter(|i| ui.checked.contains(i)).count();
    let actions = view::actions(&m, &targets, checked_count);
    let total_rate: f64 = m
        .downloads
        .keys()
        .filter(|i| m.is_running(**i))
        .filter_map(|i| {
            snaps
                .get(i)
                .map(|s| s.rate)
                .or_else(|| m.task(*i).map(|t| t.snapshot().rate))
        })
        .sum();
    let s = &m.settings;
    let columns = view::columns(&m)
        .into_iter()
        .map(|k| {
            let width = s.columns.iter().find(|c| c.0 == k).map_or(100, |c| c.2);
            let min = view::MIN_WIDTH
                .iter()
                .find(|(n, _)| *n == k)
                .map_or(0, |(_, w)| *w);
            Column {
                title: view::column_title(&k).to_uppercase(),
                width: width.max(min),
                min,
                mono: view::MONO_KEYS.contains(&k.as_str()),
                key: k,
            }
        })
        .collect();
    // The details pane: the current row if selected, else the first selected one.
    let shown = if s.show_details {
        ui.current
            .filter(|c| ui.selected.contains(c))
            .or_else(|| ui.selected.first().copied())
    } else {
        None
    };
    let details = shown.and_then(|id| {
        let d = m.downloads.get(&id)?;
        let snap = snaps
            .get(&id)
            .cloned()
            .or_else(|| m.task(id).map(|t| t.snapshot()));
        Some(view::details(&m, d, snap.as_ref()))
    });
    if ui.browser_ok.is_none() {
        let ok = state
            .sys
            .browsers()
            .iter()
            .any(|b| b.detected && b.installed);
        ui.browser_ok = Some(ok);
    }
    let menus = menu::menus(&m, &actions);
    let menus_json = serde_json::to_string(&menus).unwrap_or_default();
    let menus = if force_menus || menus_json != ui.menus_json {
        ui.menus_json = menus_json;
        Some(menus)
    } else {
        None
    };
    MainView {
        title: format!("{} {}", lider_core::APP_LONG_NAME, lider_core::VERSION),
        columns,
        rows,
        check_state: view::check_state(&visible, &ui.checked),
        tree: nodes,
        filter: ui.filter.clone(),
        actions,
        status: view::status_bar(&m, total_rate),
        browser: format!(
            "Browser: {}",
            if ui.browser_ok == Some(true) {
                "connected"
            } else {
                "not set up"
            }
        ),
        details,
        appearance: Appearance {
            theme: resolve_theme(s, ui.dark_system),
            density: s.density.clone(),
            toolbar_labels: s.toolbar_labels,
            show_categories: s.show_categories,
            show_details: s.show_details,
            show_statusbar: s.show_statusbar,
            inline_menubar: inline_menubar(s, ui.global_menu),
            list_font: s.list_font_family_and_size(),
            sidebar_width: s.sidebar_width,
            details_height: s.details_height,
            sort_key: s.sort_key.clone(),
            sort_desc: s.sort_desc,
        },
        search: SearchView {
            query: ui.search.query.clone(),
            flags: ui.search.flags,
            active: ui.search.active,
        },
        selected: ui.selected.clone(),
        current: ui.current,
        queues: view::queue_names(&m),
        menus,
    }
}

// ------------------------------------------------------------------ requests to the window

/// Something only the window (or a dialog window) can do.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiRequest {
    /// Add one address (the add flow: probe, duplicate rule, Download File Info).
    AddUrl {
        url: String,
        referer: String,
        parent_page: String,
        headers: serde_json::Map<String, Value>,
        filename: String,
        ask_address: bool,
    },
    /// Links to choose from (browser "all links", grabber, clipboard batch); the chosen ones go to the
    /// Main queue, which then starts.
    Links {
        items: Vec<(String, String, String)>,
        referer: String,
        parent_page: String,
        headers: serde_json::Map<String, Value>,
    },
    /// "Download complete" for a download.
    Complete { id: i64 },
    /// "Checksum mismatch": what to do with the file.
    Mismatch { id: i64 },
    /// The 30 s power-off countdown.
    Shutdown,
    /// Closing with downloads running: keep, background or stop.
    ConfirmClose {
        /// Each running download as (category icon, file name, "NN%" or the size so far).
        running: Vec<(String, String, String)>,
        background: bool,
        exiting: bool,
    },
    /// Offer to add LID to the application menu.
    OfferMenu,
}

pub fn request(app: &AppHandle, r: UiRequest) {
    let _ = app.emit_to("main", "lid://request", r);
}

pub fn show_normal(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

fn file_of(m: &DownloadManager, id: i64) -> String {
    m.downloads
        .get(&id)
        .map(|d| d.filename.clone())
        .unwrap_or_default()
}

/// `handle_events` of 1.x: what the window does with the manager's events.
pub fn handle_events(app: &AppHandle, state: &AppState, events: &[Event]) {
    for e in events {
        let id = e.download_id.unwrap_or(0);
        match e.kind {
            EventKind::Complete => download_complete(app, state, id),
            EventKind::Error => {
                if !crate::windows::is_open(app, &crate::windows::progress_label(id)) {
                    let m = state.manager();
                    notify(
                        app,
                        "Download failed",
                        &format!("{}\n{}", file_of(&m, id), e.message),
                    );
                }
            }
            EventKind::Mismatch => {
                crate::windows::close(app, &crate::windows::progress_label(id));
                if e.message == "kept" {
                    let m = state.manager();
                    notify(
                        app,
                        "Checksum mismatch",
                        &format!("{}\nThe file was kept.", file_of(&m, id)),
                    );
                } else {
                    request(app, UiRequest::Mismatch { id });
                }
            }
            EventKind::Changed if !e.message.is_empty() => {
                let m = state.manager();
                notify(
                    app,
                    "Checksum mismatch",
                    &format!("{}\nDownloading it again.", file_of(&m, id)),
                );
            }
            EventKind::QueueDone => {
                let name = e.queue.clone().unwrap_or_default();
                let schedule = state
                    .manager()
                    .queues
                    .get(&name)
                    .map(|q| q.schedule.clone());
                if let Some(s) = schedule {
                    let how = if e.message.is_empty() {
                        "finished"
                    } else {
                        "stopped"
                    };
                    notify(app, "LID", &format!("\"{name}\" processing {how}"));
                    if !s.open_when_done.is_empty() {
                        let _ = os().open_path(&s.open_when_done);
                    }
                    if s.shutdown_when_done {
                        show_normal(app);
                        request(app, UiRequest::Shutdown);
                    } else if s.exit_when_done {
                        quit(app, state);
                    }
                }
            }
            _ => {}
        }
    }
}

fn download_complete(app: &AppHandle, state: &AppState, id: i64) {
    let label = crate::windows::progress_label(id);
    let mut opts =
        crate::windows::completion_options(app, id).unwrap_or(crate::windows::CompletionOptions {
            show_complete: state.manager().settings.show_complete_dialog,
            exit: false,
            shutdown: false,
        });
    crate::windows::close(app, &label);
    let (in_running_queue, name) = {
        let m = state.manager();
        let d = m.downloads.get(&id);
        let q = d.map(|d| d.queue.clone()).unwrap_or_default();
        (m.queues.get(&q).is_some_and(|q| q.running), file_of(&m, id))
    };
    if in_running_queue {
        opts.show_complete = false;
    }
    if opts.show_complete {
        request(app, UiRequest::Complete { id });
    } else {
        notify(app, "Download complete", &name);
    }
    if opts.shutdown {
        show_normal(app);
        request(app, UiRequest::Shutdown);
    } else if opts.exit {
        quit(app, state);
    }
}

// ------------------------------------------------------------------ instance lines, browser, clipboard

/// A line from another start or from the browser helper.
pub fn on_line(app: &AppHandle, line: &str) {
    if line.starts_with('{') {
        if let Ok(Value::Object(req)) = serde_json::from_str::<Value>(line) {
            browser_request(app, &req);
        }
        return;
    }
    show_normal(app);
    if line != "--show" {
        request(app, add_url(line, "", "", Default::default(), "", false));
    }
}

fn add_url(
    url: &str,
    referer: &str,
    page: &str,
    headers: serde_json::Map<String, Value>,
    filename: &str,
    ask: bool,
) -> UiRequest {
    UiRequest::AddUrl {
        url: url.into(),
        referer: referer.into(),
        parent_page: page.into(),
        headers,
        filename: filename.into(),
        ask_address: ask,
    }
}

/// `handle_browser_request` of 1.x.
pub fn browser_request(app: &AppHandle, req: &serde_json::Map<String, Value>) {
    let get = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if get("action") == "show" {
        show_normal(app);
        return;
    }
    let mut headers = serde_json::Map::new();
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
                request(
                    app,
                    add_url(&url, &referer, &page, headers, &get("filename"), false),
                );
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
                    view::is_url(url).then(|| {
                        let text = i
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let kind = i
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("a")
                            .to_string();
                        (url.to_string(), text, kind)
                    })
                })
                .collect();
            if !items.is_empty() {
                show_normal(app);
                request(
                    app,
                    UiRequest::Links {
                        items,
                        referer,
                        parent_page: page,
                        headers,
                    },
                );
            }
        }
        _ => {}
    }
}

/// The clipboard monitor (1.x `_clipboard_changed`), called with the clipboard's text each tick.
pub fn clipboard_text(app: &AppHandle, state: &AppState, text: &str) {
    let text = text.trim();
    {
        // Polling stands in for QClipboard.dataChanged: what is there at startup, or while the monitor is
        // off, is not a change.
        let mut ui = state.ui();
        let first = ui.clip_seen.is_none();
        if ui.clip_seen.as_deref() == Some(text) {
            return;
        }
        ui.clip_seen = Some(text.to_string());
        if first {
            return;
        }
    }
    if !state.manager().settings.monitor_clipboard {
        return;
    }
    {
        let mut ui = state.ui();
        if text == ui.last_clip || !view::is_url(text) {
            return;
        }
        ui.last_clip = text.to_string();
    }
    let m = state.manager();
    let host = lider_core::pyutil::url_hostname(text).unwrap_or_default();
    if m.settings
        .no_auto_sites
        .split_whitespace()
        .any(|s| host.ends_with(&s.to_lowercase()))
    {
        return;
    }
    let name = lider_core::names::filename_from_url(text);
    let take = lider_core::media::is_youtube(text)
        || lider_core::categories::is_auto_capture_type(&m.settings, &name);
    drop(m);
    if take {
        request(app, add_url(text, "", "", Default::default(), "", false));
    }
}

// ------------------------------------------------------------------ saving and closing

/// Save the settings, with the window's geometry.
pub fn save_settings(app: &AppHandle, state: &AppState) {
    let mut m = state.manager();
    if let Some(w) = app.get_webview_window("main")
        && w.is_visible().unwrap_or(false)
        && let (Ok(pos), Ok(size)) = (w.outer_position(), w.inner_size())
    {
        let scale = w.scale_factor().unwrap_or(1.0);
        m.settings.window_state = Some(WindowState {
            x: (pos.x as f64 / scale) as i32,
            y: (pos.y as f64 / scale) as i32,
            width: (size.width as f64 / scale) as u32,
            height: (size.height as f64 / scale) as u32,
            maximized: w.is_maximized().unwrap_or(false),
        });
    }
    let _ = lider_core::settings::save(&m.settings, &state.dirs);
}

/// The downloads that are running or being checked, as the closing dialog lists them.
fn busy(m: &DownloadManager) -> Vec<(String, String, String)> {
    m.downloads
        .values()
        .filter(|d| m.is_running(d.id) || d.status == Status::Verifying)
        .map(|d: &Download| {
            let done = if d.size > 0 {
                format!("{:.0}%", d.progress() * 100.0)
            } else {
                lider_core::fmt::size(d.downloaded)
            };
            (view::category_icon(&d.category), d.filename.clone(), done)
        })
        .collect()
}

/// The close button or Exit. Returns true when the window may close now.
pub fn close_requested(app: &AppHandle, state: &AppState) -> bool {
    let really = state.ui().really_quit;
    let (tray, to_tray) = {
        let m = state.manager();
        (m.settings.tray_icon, m.settings.close_to_tray)
    };
    if tray && crate::tray::exists(app) && to_tray && !really {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
        return false;
    }
    let running = busy(&state.manager());
    if running.is_empty() {
        stop_and_exit(app, state);
        return true;
    }
    state.ui().really_quit = false;
    show_normal(app);
    request(
        app,
        UiRequest::ConfirmClose {
            running,
            background: !(really && crate::tray::exists(app)),
            exiting: really,
        },
    );
    false
}

/// "Exit": closing for real, also when the window would go to the tray.
pub fn quit(app: &AppHandle, state: &AppState) {
    state.ui().really_quit = true;
    if close_requested(app, state) {
        app.exit(0);
    }
}

pub fn stop_and_exit(app: &AppHandle, state: &AppState) {
    save_settings(app, state);
    crate::windows::close_all_dialogs(app);
    state.manager().shutdown();
    app.exit(0);
}

/// "Keep downloading in the background": the tray takes over.
pub fn to_background(app: &AppHandle, state: &AppState) {
    state.manager().settings.tray_icon = true;
    crate::tray::ensure(app, state);
    save_settings(app, state);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    notify(
        app,
        "LID keeps downloading",
        "Open LID from this icon. Turn the icon off again in Options → Appearance.",
    );
}
