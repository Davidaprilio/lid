//! What the window asks the backend to do (the slots of `main_window.py` in 1.x).

use lider_core::models::{MAIN_QUEUE, Status};
use lider_core::view::{self, TreeFilter};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::{self, AppState, MainView};

type Res<T> = Result<T, String>;

fn targets(state: &AppState) -> Vec<i64> {
    let ui = state.ui();
    view::targets(&ui.visible, &ui.checked, &ui.selected)
}

/// Open the progress window of a download unless Options turn it off (`force` ignores that).
pub fn show_progress(app: &AppHandle, state: &AppState, id: i64, force: bool) {
    if !force && !state.manager().settings.show_progress_dialog {
        return;
    }
    let title = state
        .manager()
        .downloads
        .get(&id)
        .map(|d| d.filename.clone())
        .unwrap_or_default();
    let _ = crate::windows::open(
        app,
        &crate::windows::progress_label(id),
        &format!("progress?id={id}"),
        &title,
        (640.0, 520.0),
        false,
    );
}

fn save(state: &AppState) {
    let m = state.manager();
    let _ = lider_core::settings::save(&m.settings, &state.dirs);
}

fn refresh_native_menu(app: &AppHandle, state: &AppState) {
    state.ui().menus_json.clear(); // the next view sends the menus again
    crate::pump::apply_menubar(app, state);
}

/// Menu items the backend handles itself; anything else goes to the window, which owns dialogs,
/// questions and the search bar.
pub fn run_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    let state = state.inner();
    let (kind, arg) = id.split_once(':').unwrap_or((id, ""));
    match kind {
        "exit" => app::quit(app, state),
        "pause_all" | "stop_all" => state.manager().stop_all(),
        "stop" => {
            let ids = targets(state);
            let mut m = state.manager();
            for i in ids {
                m.stop(i);
            }
        }
        "resume" => {
            let ids = targets(state);
            let details = state.manager().settings.show_details;
            for &i in &ids {
                let start = {
                    let m = state.manager();
                    m.downloads
                        .get(&i)
                        .is_some_and(|d| d.status != Status::Complete)
                        && !m.is_running(i)
                };
                if start {
                    state.manager().start(i);
                    if ids.len() == 1 && !details {
                        show_progress(app, state, i, false);
                    }
                }
            }
        }
        "redownload" => {
            let ids = targets(state);
            let mut m = state.manager();
            for i in ids {
                m.redownload(i, false, true);
            }
        }
        "start_queue" => {
            let events = state.manager().start_queue(arg);
            app::handle_events(app, state, &events);
        }
        "stop_queue" => state.manager().stop_queue(arg),
        "limiter_on" | "limiter_off" => {
            state
                .manager()
                .set_speed_limiter(kind == "limiter_on", None);
            save(state);
            crate::tray::ensure(app, state);
        }
        "hide_categories" => {
            let mut m = state.manager();
            m.settings.show_categories = !m.settings.show_categories;
        }
        "details" => {
            let mut m = state.manager();
            m.settings.show_details = !m.settings.show_details;
        }
        "statusbar" => {
            let mut m = state.manager();
            m.settings.show_statusbar = !m.settings.show_statusbar;
        }
        "arrange" => {
            let mut m = state.manager();
            m.settings.sort_key = arg.into();
            m.settings.sort_desc = false;
        }
        "toolbar_labels" | "toolbar_icons" => {
            state.manager().settings.toolbar_labels = kind == "toolbar_labels"
        }
        "menubar" => {
            let mode = if ["native", "custom", "both"].contains(&arg) {
                arg
            } else {
                "native"
            };
            state.manager().settings.menubar = mode.into();
            refresh_native_menu(app, state);
        }
        "theme" => {
            {
                let mut m = state.manager();
                if arg == "system" {
                    m.settings.follow_system = true;
                } else {
                    m.settings.follow_system = false;
                    m.settings.theme = arg.into();
                }
            }
            save(state);
        }
        "font_reset" => state.manager().settings.list_font.clear(),
        "add" => {
            app::show_normal(app);
            let _ = app.emit_to("main", "lid://menu", id);
        }
        _ => {
            let _ = app.emit_to("main", "lid://menu", id);
        }
    }
    crate::pump::push_view(app, state);
}

#[tauri::command]
pub fn menu_action(app: AppHandle, id: String) {
    run_menu(&app, &id);
}

#[derive(serde::Serialize)]
pub struct AppInfo {
    name: &'static str,
    long_name: &'static str,
    version: &'static str,
    mono_family: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: lider_core::APP_NAME,
        long_name: lider_core::APP_LONG_NAME,
        version: lider_core::VERSION,
        mono_family: crate::system::mono_family(),
    }
}

/// The window is ready: hand over the lines that arrived before it, offer the menu entry once.
#[tauri::command]
pub fn ready(app: AppHandle, state: State<'_, AppState>) -> MainView {
    *app::lock(&state.ready) = true;
    let lines = std::mem::take(&mut *app::lock(&state.pending));
    for l in lines {
        app::on_line(&app, &l);
    }
    let offer = std::mem::take(&mut *app::lock(&state.offer_menu));
    if offer && !state.start_hidden {
        app::request(&app, app::UiRequest::OfferMenu);
    }
    app::main_view(&state, true)
}

#[tauri::command]
pub fn view(state: State<'_, AppState>) -> MainView {
    app::main_view(&state, false)
}

#[tauri::command]
pub fn ui_filter(app: AppHandle, state: State<'_, AppState>, filter: TreeFilter) {
    {
        let mut ui = state.ui();
        ui.filter = filter;
        ui.finder.reset();
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn ui_select(app: AppHandle, state: State<'_, AppState>, ids: Vec<i64>, current: Option<i64>) {
    {
        let mut ui = state.ui();
        ui.selected = ids;
        ui.current = current;
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn ui_check(app: AppHandle, state: State<'_, AppState>, id: i64, on: bool) {
    {
        let mut ui = state.ui();
        if on {
            ui.checked.insert(id);
        } else {
            ui.checked.remove(&id);
        }
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn ui_toggle_all(app: AppHandle, state: State<'_, AppState>) {
    {
        let mut ui = state.ui();
        let visible = ui.visible.clone();
        view::toggle_all(&visible, &mut ui.checked);
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn ui_clear_checks(app: AppHandle, state: State<'_, AppState>, ids: Option<Vec<i64>>) {
    {
        let mut ui = state.ui();
        match ids {
            Some(ids) => ids.iter().for_each(|i| {
                ui.checked.remove(i);
            }),
            None => ui.checked.clear(),
        }
    }
    crate::pump::push_view(&app, &state);
}

/// A header click: the same key toggles the order, a new key sorts ascending.
#[tauri::command]
pub fn ui_header_click(app: AppHandle, state: State<'_, AppState>, key: String) {
    if !view::SORT_KEYS.contains(&key.as_str()) {
        return;
    }
    {
        let mut m = state.manager();
        if m.settings.sort_key == key {
            m.settings.sort_desc = !m.settings.sort_desc;
        } else {
            m.settings.sort_key = key;
            m.settings.sort_desc = false;
        }
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn ui_column_width(state: State<'_, AppState>, key: String, width: i64) {
    let mut m = state.manager();
    if let Some(c) = m.settings.columns.iter_mut().find(|c| c.0 == key) {
        c.2 = width;
    }
}

#[tauri::command]
pub fn ui_pane_sizes(state: State<'_, AppState>, sidebar: Option<i64>, details: Option<i64>) {
    let mut m = state.manager();
    if let Some(w) = sidebar {
        m.settings.sidebar_width = w;
    }
    if let Some(h) = details {
        m.settings.details_height = h;
    }
}

/// "Find": filter the list by the query (an empty query shows everything).
#[tauri::command]
pub fn ui_search(app: AppHandle, state: State<'_, AppState>, query: String) -> Res<()> {
    let result = {
        let mut ui = state.ui();
        ui.search.query = query.clone();
        ui.search.active = !query.is_empty();
        let r = ui.search.validate().map_err(|e| e.to_string());
        let s = ui.search.clone();
        ui.finder = lider_core::search::FindNext::new(s);
        r
    };
    persist_search(&state);
    crate::pump::push_view(&app, &state);
    result
}

fn persist_search(state: &AppState) {
    let (q, f) = {
        let ui = state.ui();
        (ui.search.query.clone(), ui.search.flags)
    };
    let mut m = state.manager();
    m.settings.search_string = q;
    m.settings.search_flags = f;
    let _ = lider_core::settings::save(&m.settings, &state.dirs);
}

#[tauri::command]
pub fn ui_search_close(app: AppHandle, state: State<'_, AppState>) {
    state.ui().search.active = false;
    crate::pump::push_view(&app, &state);
}

/// The search settings (targets, match case, whole string).
#[tauri::command]
pub fn ui_search_flags(
    app: AppHandle,
    state: State<'_, AppState>,
    flags: i64,
    query: Option<String>,
) {
    {
        let mut ui = state.ui();
        ui.search.flags = flags & 0x1F;
        if let Some(q) = query {
            ui.search.query = q;
        }
        let s = ui.search.clone();
        ui.finder.settings = s;
    }
    persist_search(&state);
    crate::pump::push_view(&app, &state);
}

/// "Find Next": the id of the next matching row, `None` for "No such files found".
#[tauri::command]
pub fn ui_find_next(app: AppHandle, state: State<'_, AppState>) -> Res<Option<i64>> {
    let rows: Vec<lider_core::models::Download> = {
        // Always the manager first, then the window state (the same order as the pump).
        let m = state.manager();
        let ui = state.ui();
        ui.visible
            .iter()
            .filter_map(|i| m.downloads.get(i).cloned())
            .collect()
    };
    let found = {
        let mut ui = state.ui();
        let s = ui.search.clone();
        ui.finder.settings.query = s.query;
        ui.finder.settings.flags = s.flags;
        ui.finder.next(&rows).map_err(|e| e.to_string())?
    };
    let id = found.map(|i| rows[i].id);
    if let Some(id) = id {
        let mut ui = state.ui();
        ui.selected = vec![id];
        ui.current = Some(id);
    }
    persist_search(&state);
    crate::pump::push_view(&app, &state);
    Ok(id)
}

#[derive(serde::Serialize)]
pub struct Targets {
    ids: Vec<i64>,
    completed: usize,
}

#[tauri::command]
pub fn targets_info(state: State<'_, AppState>) -> Targets {
    let ids = targets(&state);
    let completed = state
        .manager()
        .downloads
        .values()
        .filter(|d| d.status == Status::Complete)
        .count();
    Targets { ids, completed }
}

#[tauri::command]
pub fn act_remove(app: AppHandle, state: State<'_, AppState>, ids: Vec<i64>, delete_files: bool) {
    state.manager().remove(&ids, delete_files);
    {
        let mut ui = state.ui();
        for i in &ids {
            ui.checked.remove(i);
        }
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn act_delete_completed(app: AppHandle, state: State<'_, AppState>) -> usize {
    let n = state.manager().remove_completed().len();
    crate::pump::push_view(&app, &state);
    n
}

/// Double-click: a finished download opens Properties, any other one starts and shows its progress.
#[tauri::command]
pub fn act_activate(app: AppHandle, state: State<'_, AppState>, id: i64) {
    let complete = state
        .manager()
        .downloads
        .get(&id)
        .is_some_and(|d| d.status == Status::Complete);
    if complete {
        let _ = crate::windows::open(
            &app,
            &format!("properties-{id}"),
            &format!("properties?id={id}"),
            "File Properties",
            (560.0, 600.0),
            false,
        );
        return;
    }
    if !state.manager().is_running(id) {
        state.manager().start(id);
    }
    show_progress(&app, &state, id, false);
    crate::pump::push_view(&app, &state);
}

/// The details pane's first button: Pause, Skip check, Download again, Open or Resume.
#[tauri::command]
pub fn act_details_button(app: AppHandle, state: State<'_, AppState>, id: i64, action: String) {
    match action.as_str() {
        "pause" => state.manager().stop(id),
        "skip" => state.manager().skip_check(id),
        "again" => state.manager().redownload(id, true, true),
        "open" => {
            let p = state.manager().downloads.get(&id).map(|d| d.path());
            if let Some(p) = p {
                let _ = crate::system::open_path(&p);
            }
        }
        _ => {
            state.manager().start(id);
        }
    }
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn act_open_window(app: AppHandle, state: State<'_, AppState>, id: i64) {
    show_progress(&app, &state, id, true);
}

#[tauri::command]
pub fn act_download_limit(state: State<'_, AppState>, id: i64, kbps: i64) {
    state.manager().set_download_speed_limit(id, kbps);
}

#[tauri::command]
pub fn act_open(state: State<'_, AppState>, id: i64) -> Res<()> {
    let p = state
        .manager()
        .downloads
        .get(&id)
        .map(|d| d.path())
        .ok_or("no such download")?;
    crate::system::open_path(&p).map_err(|e| e.to_string())
}

/// "Open folder": the file selected in the file manager when complete, else its folder.
#[tauri::command]
pub fn act_open_folder(state: State<'_, AppState>, id: i64) -> Res<()> {
    let d = state
        .manager()
        .downloads
        .get(&id)
        .cloned()
        .ok_or("no such download")?;
    let r = if d.status == Status::Complete {
        crate::system::open_folder(&d.path())
    } else {
        crate::system::open_path(&d.save_dir)
    };
    r.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn act_open_with(state: State<'_, AppState>, id: i64, command: String) -> Res<()> {
    let p = state
        .manager()
        .downloads
        .get(&id)
        .map(|d| d.path())
        .ok_or("no such download")?;
    crate::system::open_with(&command, &p).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn act_set_queue(app: AppHandle, state: State<'_, AppState>, ids: Vec<i64>, name: String) {
    state.manager().set_queue(&ids, &name);
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn act_queue(app: AppHandle, state: State<'_, AppState>, name: String, start: bool) {
    if start {
        let events = state.manager().start_queue(&name);
        app::handle_events(&app, &state, &events);
    } else {
        state.manager().stop_queue(&name);
    }
    crate::pump::push_view(&app, &state);
}

/// Batch, drop and import: add to the Main queue. Returns how many (the window then asks to start).
#[tauri::command]
pub fn act_add_many(
    app: AppHandle,
    state: State<'_, AppState>,
    items: Vec<(String, String, String)>,
    username: Option<String>,
    password: Option<String>,
) -> usize {
    let n = view::add_many(
        &mut state.manager(),
        &items,
        username.as_deref().unwrap_or(""),
        password.as_deref().unwrap_or(""),
    );
    crate::pump::push_view(&app, &state);
    n
}

#[tauri::command]
pub fn act_start_main_queue(app: AppHandle, state: State<'_, AppState>) {
    let events = state.manager().start_queue(MAIN_QUEUE);
    app::handle_events(&app, &state, &events);
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn copy_text(text: String) -> Res<()> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_text() -> String {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .unwrap_or_default()
}

/// Tasks → Export: `kind` "ef2" or "text".
#[tauri::command]
pub fn export_list(state: State<'_, AppState>, path: String, kind: String) -> Res<()> {
    let m = state.manager();
    let text = if kind == "ef2" {
        lider_core::ef2::export_ef2(m.downloads.values())
    } else {
        lider_core::ef2::export_text(m.downloads.values())
    };
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Tasks → Import: the (url, referer, description) found in the file.
#[tauri::command]
pub fn import_list(path: String, kind: String) -> Res<Vec<(String, String, String)>> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(if kind == "ef2" {
        lider_core::ef2::parse_ef2(&text)
            .into_iter()
            .map(|(u, r)| (u, r, String::new()))
            .collect()
    } else {
        lider_core::ef2::urls_in(&text)
            .into_iter()
            .map(|u| (u, String::new(), String::new()))
            .collect()
    })
}

#[tauri::command]
pub fn set_list_font(app: AppHandle, state: State<'_, AppState>, family: String, size: f64) {
    state.manager().settings.list_font = format!("{family},{size}");
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn system_fonts() -> Vec<String> {
    crate::system::font_families()
}

/// The answer to "downloads are running" when closing: keep, background or stop.
#[tauri::command]
pub fn close_choice(app: AppHandle, state: State<'_, AppState>, choice: String) {
    match choice.as_str() {
        "background" => app::to_background(&app, &state),
        "stop" => app::stop_and_exit(&app, &state),
        _ => {}
    }
}

/// The power-off countdown reached 0.
#[tauri::command]
pub fn shutdown_now(app: AppHandle, state: State<'_, AppState>) {
    app::save_settings(&app, &state);
    state.manager().shutdown();
    let _ = crate::system::power_off();
}

/// The answer to "Add LID to the application menu?" (asked once either way).
#[tauri::command]
pub fn menu_offer_answer(state: State<'_, AppState>, add: bool) -> Res<()> {
    {
        let mut m = state.manager();
        m.settings.menu_offered = true;
        let _ = lider_core::settings::save(&m.settings, &state.dirs);
    }
    if add {
        lider_sys::desktop::add_to_menu(&state.sys).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The status bar's "Browser: …" is checked again (after Browser integration).
#[tauri::command]
pub fn recheck_browser(app: AppHandle, state: State<'_, AppState>) {
    state.ui().browser_ok = None;
    crate::pump::push_view(&app, &state);
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>) {
    app::save_settings(&app, &state);
}

/// A browser request handed over by a dialog or a test.
#[tauri::command]
pub fn browser_request(app: AppHandle, request: Value) {
    if let Value::Object(r) = request {
        app::browser_request(&app, &r);
    }
}

/// A progress window says what to do when its download completes.
#[tauri::command]
pub fn set_completion_options(
    window: tauri::WebviewWindow,
    options: crate::windows::CompletionOptions,
) {
    crate::windows::set_completion_options(window.label(), options);
}
