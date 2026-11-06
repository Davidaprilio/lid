//! Commands of LID's dialog windows (`ui/dialogs/*.py` in 1.x). Each dialog loads what it shows with one
//! call and applies its result with another; rules that decide something live here, not in the page.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use lider_core::checksum::{self, Verifier};
use lider_core::fmt;
use lider_core::manager::NewDownload;
use lider_core::media::{self, MediaInfo, Variant};
use lider_core::models::{Download, MAIN_QUEUE, SYNC_QUEUE, Status};
use lider_core::queue::QueueSchedule;
use lider_core::settings::{Category, ColumnState, Settings};
use lider_core::view;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tauri::{AppHandle, Manager, State};

use crate::app::{self, AppState};

type Res<T> = Result<T, String>;

fn push(app: &AppHandle, state: &AppState) {
    crate::pump::push_view(app, state);
}

fn save(state: &AppState) {
    let m = state.manager();
    let _ = lider_core::settings::save(&m.settings, &state.dirs);
}

/// "12.3 MB (12,345,678 Bytes)".
fn size_with_bytes(n: i64) -> String {
    let digits = n.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{} ({grouped} Bytes)", fmt::size(n))
}

// ------------------------------------------------------------------ shared

#[tauri::command]
pub fn dialog_theme(state: State<'_, AppState>) -> String {
    let m = state.manager();
    app::resolve_theme(&m.settings, state.ui().dark_system)
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Value {
    state.manager().settings.to_json()
}

/// Change some settings (keys as in settings.json) and apply them everywhere.
#[tauri::command]
pub fn settings_patch(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: Map<String, Value>,
) -> Res<()> {
    {
        let mut m = state.manager();
        let mut current = m.settings.to_json();
        if let Value::Object(c) = &mut current {
            for (k, v) in patch {
                c.insert(k, v);
            }
        }
        let mut next = Settings::from_json(&current, &state.dirs).map_err(|e| e.to_string())?;
        next.window_state = m.settings.window_state;
        m.settings = next;
        let on = m.settings.speed_limiter_on;
        m.set_speed_limiter(on, None);
        let _ = lider_core::settings::save(&m.settings, &state.dirs);
    }
    crate::tray::ensure(&app, &state);
    state.ui().menus_json.clear();
    crate::pump::apply_menubar(&app, &state);
    Ok(())
}

#[derive(Serialize)]
pub struct Parsed {
    value: String,
    title: String,
    error: String,
}

/// What the checksum field shows: the parsed value and its type, or why it is not one.
#[tauri::command]
pub fn checksum_parse(text: String, algorithm: String) -> Parsed {
    match checksum::parse(&text, &algorithm) {
        Ok(v) => Parsed {
            title: checksum::title(&v).into(),
            value: v,
            error: String::new(),
        },
        Err(e) => Parsed {
            value: String::new(),
            title: String::new(),
            error: e.0,
        },
    }
}

// ------------------------------------------------------------------ limiter, columns, categories

#[tauri::command]
pub fn limiter_apply(app: AppHandle, state: State<'_, AppState>, kbps: i64, on_startup: bool) {
    {
        let mut m = state.manager();
        m.settings.speed_limit_kbps = kbps.clamp(1, 10_000_000);
        m.settings.speed_limiter_on_startup = on_startup;
        let on = m.settings.speed_limiter_on;
        m.set_speed_limiter(on, None);
    }
    save(&state);
    push(&app, &state);
}

/// Columns in list order; "filename" always shows.
#[tauri::command]
pub fn columns_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    columns: Option<Vec<ColumnState>>,
) {
    {
        let mut m = state.manager();
        m.settings.columns = match columns {
            Some(mut c) => {
                for col in &mut c {
                    if col.0 == "filename" {
                        col.1 = true;
                    }
                }
                c
            }
            None => lider_core::settings::DEFAULT_COLUMNS
                .iter()
                .map(|c| ColumnState(c.key.into(), c.visible, c.width))
                .collect(),
        };
    }
    save(&state);
    push(&app, &state);
}

/// Add a category (`old` = None; ignored when the name exists) or replace one; a rename moves its downloads.
#[tauri::command]
pub fn category_save(
    app: AppHandle,
    state: State<'_, AppState>,
    old: Option<String>,
    category: Category,
) -> bool {
    let added = {
        let mut m = state.manager();
        match &old {
            None => {
                if m.settings
                    .categories
                    .iter()
                    .any(|c| c.name == category.name)
                {
                    false
                } else {
                    m.settings.categories.push(category.clone());
                    true
                }
            }
            Some(name) => {
                if let Some(i) = m.settings.categories.iter().position(|c| &c.name == name) {
                    m.settings.categories[i] = category.clone();
                }
                if &category.name != name {
                    let ids: Vec<i64> = m
                        .downloads
                        .values()
                        .filter(|d| &d.category == name)
                        .map(|d| d.id)
                        .collect();
                    let new = category.name.clone();
                    for id in ids {
                        m.edit(id, |d| d.category = new.clone());
                    }
                }
                true
            }
        }
    };
    save(&state);
    push(&app, &state);
    added
}

// ------------------------------------------------------------------ adding downloads

#[tauri::command]
pub fn url_history(state: State<'_, AppState>) -> Vec<String> {
    state.ui().url_history.clone()
}

/// Most recent first, no repeats, at most 20.
#[tauri::command]
pub fn remember_url(state: State<'_, AppState>, url: String) {
    let mut ui = state.ui();
    ui.url_history.retain(|u| u != &url);
    ui.url_history.insert(0, url);
    ui.url_history.truncate(20);
}

#[derive(Serialize)]
pub struct DuplicateInfo {
    id: i64,
    complete: bool,
}

#[derive(Serialize)]
pub struct AddCheck {
    duplicate: Option<DuplicateInfo>,
    /// ask | number | overwrite | complete
    action: String,
    youtube: bool,
    yt_dlp: bool,
    show_start_dialog: bool,
    only_add_to_queue: bool,
}

#[tauri::command]
pub fn add_check(state: State<'_, AppState>, url: String) -> AddCheck {
    let m = state.manager();
    AddCheck {
        duplicate: m.find_duplicate(&url).map(|d| DuplicateInfo {
            id: d.id,
            complete: d.status == Status::Complete,
        }),
        action: m.settings.duplicate_action.clone(),
        youtube: media::is_youtube(&url),
        yt_dlp: media::available(),
        show_start_dialog: m.settings.show_start_dialog,
        only_add_to_queue: m.settings.only_add_to_queue,
    }
}

/// The duplicate rule "complete": a finished duplicate shows Download complete, any other one starts.
#[tauri::command]
pub fn add_duplicate_existing(app: AppHandle, state: State<'_, AppState>, id: i64) {
    let complete = state
        .manager()
        .downloads
        .get(&id)
        .is_some_and(|d| d.status == Status::Complete);
    if complete {
        app::request(&app, app::UiRequest::Complete { id });
    } else {
        state.manager().start(id);
        crate::commands::show_progress(&app, &state, id, false);
    }
    push(&app, &state);
}

#[derive(Serialize)]
pub struct Probe {
    url: String,
    size: i64,
    resumable: bool,
    filename: String,
    content_type: String,
}

/// Ask the server about an address (size, resume, file name).
#[tauri::command]
pub async fn probe_url(
    state: State<'_, AppState>,
    url: String,
    referer: String,
    username: String,
    password: String,
    headers: indexmap::IndexMap<String, String>,
) -> Res<Probe> {
    let fut = state
        .manager()
        .probe(&url, &referer, &username, &password, headers);
    let handle = state._engine.handle().clone();
    let p = handle
        .spawn(fut)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.message())?;
    Ok(Probe {
        url: p.url,
        size: p.size,
        resumable: p.resumable,
        filename: p.filename,
        content_type: p.content_type,
    })
}

/// Read a YouTube page with yt-dlp.
#[tauri::command]
pub async fn probe_media(url: String) -> Res<MediaInfo> {
    tauri::async_runtime::spawn_blocking(move || media::probe(&url))
        .await
        .map_err(|e| e.to_string())?
}

/// The file name to use: the probed one; the browser's when it has a dot and the probed one has none;
/// a name from the address when the probe failed (`probed` None).
#[tauri::command]
pub fn name_choice(url: String, probed: Option<String>, browser: String) -> String {
    use lider_core::names::{filename_from_url, safe_filename};
    match probed {
        None => safe_filename(&if browser.is_empty() {
            filename_from_url(&url)
        } else {
            browser
        }),
        Some(name) if browser.contains('.') && !name.contains('.') => safe_filename(&browser),
        Some(name) => name,
    }
}

/// The variant to preselect and the file name for it.
#[tauri::command]
pub fn media_default(state: State<'_, AppState>, info: MediaInfo) -> Option<(Variant, String)> {
    let (kind, quality) = {
        let m = state.manager();
        (
            m.settings.media_kind.clone(),
            m.settings.media_quality.clone(),
        )
    };
    let v = info.default_variant(&kind, &quality)?.clone();
    let name = media::suggested_filename(&info, &v);
    Some((v, name))
}

/// The quality preselected when the user switches between Video and Audio only (1.x `_fill_quality`).
#[tauri::command]
pub fn media_variant(info: MediaInfo, kind: String) -> Option<Variant> {
    info.default_variant(&kind, "").cloned()
}

#[derive(Serialize)]
pub struct Prepared {
    filename: String,
    category: String,
    save_dir: String,
}

/// The category, folder and a free name for a new download (`_confirm_and_add` in 1.x).
#[tauri::command]
pub fn add_prepare(
    state: State<'_, AppState>,
    url: String,
    filename: String,
    replacing: bool,
) -> Prepared {
    let m = state.manager();
    let cat = lider_core::categories::classify(&m.settings, &filename, &url).clone();
    let name = if replacing {
        filename
    } else {
        view::unique_name(&m, &cat.save_dir, &filename)
    };
    Prepared {
        filename: name,
        category: cat.name,
        save_dir: cat.save_dir,
    }
}

#[derive(Deserialize)]
pub struct AddRequest {
    url: String,
    path: String,
    category: String,
    description: String,
    referer: String,
    parent_page: String,
    username: String,
    password: String,
    size: i64,
    resumable: Option<bool>,
    headers: indexmap::IndexMap<String, String>,
    checksum: String,
    /// start | later
    action: String,
    queue: String,
    /// The duplicate this one replaces ("overwrite").
    replace: Option<i64>,
    remember_path: bool,
    media: Option<(MediaInfo, Variant)>,
}

/// Add the download as Download File Info decided it; it starts (with its progress window) unless "later".
#[tauri::command]
pub fn add_download(app: AppHandle, state: State<'_, AppState>, req: AddRequest) -> Res<i64> {
    let path = std::path::Path::new(&req.path);
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or("no file name")?;
    let save_dir = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let start = req.action == "start";
    let id = {
        let mut m = state.manager();
        if req.remember_path {
            let dir = save_dir.clone();
            m.settings.category_mut(&req.category).save_dir = dir;
        }
        if let Some((_, v)) = &req.media {
            m.settings.media_kind = v.kind.clone();
            m.settings.media_quality = v.quality.clone();
        }
        if let Some(old) = req.replace {
            let same = m
                .downloads
                .get(&old)
                .filter(|d| d.status == Status::Complete && d.path() == req.path)
                .is_some();
            m.remove(&[old], same);
        }
        let media = req
            .media
            .as_ref()
            .map(|(info, v)| media::media_record(info, v));
        let queue = if start || req.action != "later" {
            String::new()
        } else {
            req.queue.clone()
        };
        let d = m.add(
            &req.url,
            &filename,
            NewDownload {
                save_dir: Some(save_dir),
                category: Some(req.category.clone()),
                description: req.description,
                referer: req.referer,
                parent_page: req.parent_page,
                username: req.username,
                password: req.password,
                size: Some(req.size),
                resumable: req.resumable,
                queue,
                start,
                media,
                headers: req.headers,
                checksum: req.checksum,
            },
        );
        let _ = lider_core::settings::save(&m.settings, &state.dirs);
        d.id
    };
    if start {
        crate::commands::show_progress(&app, &state, id, false);
    }
    push(&app, &state);
    Ok(id)
}

/// Batch download: the addresses for the pattern.
#[tauri::command]
pub fn batch_preview(pattern: String, fill: lider_core::batch::Fill) -> Vec<String> {
    lider_core::batch::expand(&pattern, &fill)
}

#[derive(Serialize)]
pub struct Page {
    base: String,
    links: Vec<lider_core::links::Link>,
}

/// The grabber: fetch one page (at most 8 MiB) and list its links.
#[tauri::command]
pub async fn grab_page(state: State<'_, AppState>, url: String) -> Res<Page> {
    let client = state.manager().client().map_err(|e| e.message())?;
    let handle = state._engine.handle().clone();
    handle
        .spawn(async move {
            let mut r = client
                .open(&url, 0, false, &Default::default())
                .await
                .map_err(|e| e.message())?;
            let charset = r
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .and_then(|ct| {
                    ct.split(';').find_map(|p| {
                        p.trim()
                            .strip_prefix("charset=")
                            .map(|c| c.trim_matches('"').to_string())
                    })
                })
                .unwrap_or_else(|| "utf-8".into());
            let mut body = Vec::new();
            while body.len() < 8 * 1024 * 1024 {
                match r
                    .body
                    .chunk(client.timeout)
                    .await
                    .map_err(|e| e.message())?
                {
                    Some(c) => body.extend_from_slice(&c),
                    None => break,
                }
            }
            body.truncate(8 * 1024 * 1024);
            let enc =
                encoding_rs::Encoding::for_label(charset.as_bytes()).unwrap_or(encoding_rs::UTF_8);
            let text = enc.decode_without_bom_handling(&body).0.into_owned();
            Ok(Page {
                links: lider_core::links::extract_links(&text, &r.url),
                base: r.url,
            })
        })
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn links_rows(
    state: State<'_, AppState>,
    links: Vec<lider_core::links::Link>,
    filters: lider_core::links::Filters,
) -> Vec<lider_core::links::LinkRow> {
    lider_core::links::rows(&state.manager().settings, &links, filters)
}

#[derive(Deserialize)]
pub struct Choice {
    url: String,
    filename: String,
    description: String,
    category: String,
    save_dir: String,
}

/// Links chosen in "Download All Links": a free name each, all in the Main queue, which then starts.
#[tauri::command]
pub fn add_choices(
    app: AppHandle,
    state: State<'_, AppState>,
    choices: Vec<Choice>,
    referer: String,
    parent_page: String,
    headers: indexmap::IndexMap<String, String>,
) -> usize {
    let n = choices.len();
    {
        let mut m = state.manager();
        for c in choices {
            let name = view::unique_name(&m, &c.save_dir, &c.filename);
            m.add(
                &c.url,
                &name,
                NewDownload {
                    save_dir: Some(c.save_dir),
                    category: Some(c.category),
                    description: c.description,
                    referer: referer.clone(),
                    parent_page: parent_page.clone(),
                    headers: headers.clone(),
                    queue: MAIN_QUEUE.into(),
                    ..Default::default()
                },
            );
        }
    }
    if n > 0 {
        let events = state.manager().start_queue(MAIN_QUEUE);
        app::handle_events(&app, &state, &events);
    }
    push(&app, &state);
    n
}

// ------------------------------------------------------------------ progress, complete, mismatch

fn pill(text: impl Into<String>, tone: &str) -> Value {
    json!({"text": text.into(), "tone": tone})
}

/// Everything the progress window shows (refreshed every 500 ms).
#[tauri::command]
pub fn progress_info(state: State<'_, AppState>, id: i64) -> Option<Value> {
    let m = state.manager();
    let d = m.get(id)?;
    let task = m.task(id);
    let running = m.is_running(id);
    let snap = task.as_ref().map(|t| t.snapshot());
    let status_text = task.as_ref().map(|t| t.status_text()).unwrap_or_default();
    let verify = m.verify_progress(id);
    let status = if let Some(v) = verify {
        pill(format!("Verifying {:.0}%", v * 100.0), "accent")
    } else if !status_text.is_empty() {
        pill(status_text, "accent")
    } else if d.status == Status::Error {
        pill(format!("Error: {}", d.error), "bad")
    } else if d.status == Status::Paused {
        pill("Paused", "muted")
    } else if d.status == Status::Downloading {
        pill("⤓ Receiving data…", "accent")
    } else if d.status == Status::Assembling {
        pill("Assembling…", "accent")
    } else if d.status == Status::Complete {
        pill("✓ Complete", "ok")
    } else {
        pill("Connecting…", "accent")
    };
    let quality = d
        .media
        .as_ref()
        .and_then(|md| md.get("variant"))
        .map(|v| {
            format!(
                "   ({} {})",
                v["quality"].as_str().unwrap_or(""),
                v["ext"].as_str().unwrap_or("").to_uppercase()
            )
        })
        .unwrap_or_default();
    let rate = snap.as_ref().map_or(0.0, |s| s.rate);
    let left = snap.as_ref().map_or(-1, |s| s.time_left);
    let conns: Vec<Value> = snap
        .as_ref()
        .map(|s| {
            s.connections
                .iter()
                .map(|c| {
                    json!({"n": c.index.to_string(), "downloaded": fmt::size(c.received),
                        "speed": if c.alive && running { fmt::rate(c.rate) } else { String::new() },
                        "info": if c.alive || running { c.status.clone() } else { String::new() }})
                })
                .collect()
        })
        .unwrap_or_default();
    let percent = if d.size > 0 {
        format!("{:.1}%", d.progress() * 100.0)
    } else {
        fmt::size(d.downloaded)
    };
    let button = if d.status == Status::Verifying {
        json!({"label": "Skip check", "action": "skip", "icon": ""})
    } else if running {
        json!({"label": "Pause", "action": "pause", "icon": "stop"})
    } else {
        json!({"label": "Resume", "action": "resume", "icon": "resume"})
    };
    Some(json!({
        "title": format!("{} {}", if d.size > 0 { format!("{:.0}%", d.progress() * 100.0) } else { fmt::size(d.downloaded) }, d.filename),
        "url": format!("{}{quality}", d.url),
        "status": status,
        "size": if d.size >= 0 { fmt::size(d.size) } else { "(unknown)".into() },
        "downloaded": if d.size > 0 { format!("{} ({:.1} %)", fmt::size(d.downloaded), d.progress() * 100.0) } else { fmt::size(d.downloaded) },
        "rate": if running { fmt::rate(rate) } else { String::new() },
        "time_left": if running && left >= 0 { fmt::duration(left) } else { "(unknown)".into() },
        "resume": match d.resumable { Some(true) => pill("✓ Yes", "ok"), Some(false) => pill("No", "muted"), None => pill("(unknown)", "muted") },
        "percent": percent,
        "bar": d.progress(),
        "size_total": d.size,
        "segments": snap.as_ref().map(|s| s.segments.clone()).unwrap_or_else(|| d.segments.clone()),
        "connections": conns,
        "button": button,
        "complete": d.status == Status::Complete,
        "limit_on": d.speed_limit_kbps > 0,
        "limit_kbps": if d.speed_limit_kbps > 0 { d.speed_limit_kbps } else { m.settings.speed_limit_kbps },
        "show_complete": m.settings.show_complete_dialog,
        "path": d.path(),
    }))
}

/// The progress window's speed limit: applied at once; kept on the download when `remember`.
#[tauri::command]
pub fn progress_limit(state: State<'_, AppState>, id: i64, kbps: i64, remember: bool) {
    let mut m = state.manager();
    if let Some(t) = m.task(id) {
        t.set_speed_limit(kbps);
    }
    if remember {
        m.edit(id, |d| d.speed_limit_kbps = kbps);
    }
}

/// Pause, Resume or Skip check.
#[tauri::command]
pub fn progress_button(app: AppHandle, state: State<'_, AppState>, id: i64, action: String) {
    match action.as_str() {
        "skip" => state.manager().skip_check(id),
        "pause" => state.manager().stop(id),
        _ => {
            state.manager().start(id);
        }
    }
    push(&app, &state);
}

#[tauri::command]
pub fn complete_info(state: State<'_, AppState>, id: i64) -> Option<Value> {
    let m = state.manager();
    let d = m.downloads.get(&id)?;
    Some(json!({
        "filename": d.filename,
        "url": d.url,
        "path": d.path(),
        "icon": view::category_icon(&d.category),
        "downloaded": format!("Downloaded {}", size_with_bytes(d.size.max(d.downloaded))),
        "verified": d.checksum_result == "verified",
        "kind": checksum::title(&d.checksum),
        "actual": d.checksum_actual,
        "expected": checksum::split(&d.checksum).1,
        "size": fmt::size(d.size),
    }))
}

#[tauri::command]
pub fn complete_dont_show(window: tauri::WebviewWindow, on: bool) {
    let mut set = DONT_SHOW.lock().unwrap_or_else(|e| e.into_inner());
    if on {
        set.insert(window.label().to_owned());
    } else {
        set.remove(window.label());
    }
}

/// Complete windows whose "Don't show this dialog again" is checked; applied when the window closes,
/// whichever way (1.x `_complete_closed`).
static DONT_SHOW: LazyLock<Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);

/// A dialog window went away (also through its title bar).
pub fn window_closed(app: &AppHandle, label: &str) {
    stop_checks_of(label);
    if DONT_SHOW
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(label)
    {
        let state = app.state::<AppState>();
        state.manager().settings.show_complete_dialog = false;
        save(&state);
        push(app, &state);
    }
}

/// What to do with a file that does not match its checksum: again, delete (to the Trash) or keep.
#[tauri::command]
pub fn mismatch_choice(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    choice: String,
) -> Res<()> {
    match choice.as_str() {
        "again" => {
            state.manager().redownload(id, true, true);
            crate::commands::show_progress(&app, &state, id, false);
        }
        "delete" => {
            let path = state.manager().downloads.get(&id).map(|d| d.path());
            if let Some(p) = path
                && std::path::Path::new(&p).exists()
            {
                trash::delete(&p).map_err(|e| e.to_string())?;
            }
            state.manager().remove(&[id], false);
            state.ui().checked.remove(&id);
        }
        _ => {}
    }
    push(&app, &state);
    Ok(())
}

// ------------------------------------------------------------------ check hash

/// Running checks by token, with the label of the window that started them.
type Checks = HashMap<u64, (Arc<Verifier>, String)>;
static CHECKS: LazyLock<Mutex<Checks>> = LazyLock::new(Default::default);
static NEXT: AtomicU64 = AtomicU64::new(1);

/// The menu item "Check hash…" is enabled: finished, being checked, has data or runs.
pub fn can_check(m: &lider_core::manager::DownloadManager, d: &Download) -> bool {
    matches!(d.status, Status::Complete | Status::Verifying)
        || d.downloaded > 0
        || m.is_running(d.id)
}

#[tauri::command]
pub fn checkhash_info(state: State<'_, AppState>, id: i64) -> Option<Value> {
    let m = state.manager();
    let d = m.get(id)?;
    let finished = matches!(d.status, Status::Complete | Status::Verifying);
    let subtitle = if finished || d.size <= 0 {
        let n = if d.size > 0 { d.size } else { d.downloaded };
        format!("{} · {}", d.filename, fmt::size(n))
    } else {
        // "12.3 of 40 MB": the number of the downloaded part without its unit.
        let done = fmt::size(d.downloaded);
        let number = done
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        format!("{} · {} of {}", d.filename, number, fmt::size(d.size))
    };
    Some(json!({
        "filename": d.filename,
        "subtitle": subtitle,
        "checksum": d.checksum,
        "finished": finished,
        "exists": std::path::Path::new(&d.path()).is_file(),
        "path": d.path(),
        "size": d.size,
        "percent": if d.size > 0 { format!("{:.0}%", d.progress() * 100.0) } else { String::new() },
        "can_check": can_check(&m, &d),
    }))
}

/// Start reading the file; `checksum` is "algo:hex" or "algo:" to only compute. Returns a token.
#[tauri::command]
pub fn checkhash_start(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    id: i64,
    checksum: String,
) -> Res<u64> {
    let path = state
        .manager()
        .downloads
        .get(&id)
        .map(|d| d.path())
        .ok_or("no such download")?;
    let v = Arc::new(Verifier::new(path, &checksum, true, id));
    let token = NEXT.fetch_add(1, Ordering::Relaxed);
    lock_checks().insert(token, (v.clone(), window.label().to_owned()));
    std::thread::Builder::new()
        .name(format!("checkhash-{id}"))
        .spawn(move || {
            v.run();
        })
        .map_err(|e| e.to_string())?;
    Ok(token)
}

fn lock_checks() -> std::sync::MutexGuard<'static, Checks> {
    CHECKS.lock().unwrap_or_else(|e| e.into_inner())
}

#[tauri::command]
pub fn checkhash_poll(token: u64) -> Option<Value> {
    let v = lock_checks().get(&token).map(|c| c.0.clone())?;
    let out = v.outcome();
    let total = std::fs::metadata(&v.path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let progress = v.progress();
    Some(json!({
        "progress": progress,
        "read": fmt::size((total as f64 * progress) as i64),
        "total": fmt::size(total),
        "done": out.is_some(),
        "result": out.as_ref().map(|o| o.result.as_str()).unwrap_or(""),
        "actual": out.as_ref().map(|o| o.actual.clone()).unwrap_or_default(),
        "error": out.as_ref().map(|o| o.error.clone()).unwrap_or_default(),
        "seconds": out.as_ref().map(|o| o.seconds).unwrap_or(0.0),
        "algorithm": checksum::algorithm_title(&v.algorithm),
    }))
}

/// Stop reading (Stop, or the window closes); nothing is saved.
#[tauri::command]
pub fn checkhash_stop(token: u64) {
    if let Some((v, _)) = lock_checks().remove(&token) {
        v.skip();
    }
}

/// A Check hash window went away (also through its title bar): stop what it was reading, like 1.x `done()`.
pub fn stop_checks_of(label: &str) {
    lock_checks().retain(|_, (v, owner)| {
        let keep = owner != label;
        if !keep {
            v.skip();
        }
        keep
    });
}

#[tauri::command]
pub fn checkhash_save(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    checksum: String,
    result: String,
    actual: String,
    seconds: f64,
) {
    state
        .manager()
        .record_check(id, &checksum, &result, &actual, seconds);
    push(&app, &state);
}

#[tauri::command]
pub fn checkhash_later(app: AppHandle, state: State<'_, AppState>, id: i64, checksum: String) {
    state.manager().check_when_finished(id, &checksum);
    push(&app, &state);
}

// ------------------------------------------------------------------ properties

#[tauri::command]
pub fn properties_info(state: State<'_, AppState>, id: i64) -> Option<Value> {
    let m = state.manager();
    let d = m.get(id)?;
    let running = m.is_running(id);
    let ext = lider_core::categories::extension(&d.filename);
    let mime = mime_guess::from_path(&d.filename)
        .first_raw()
        .map(String::from);
    let kind = match (mime, ext.is_empty()) {
        (Some(mt), _) => mt,
        (None, false) => ext.to_uppercase(),
        (None, true) => "Unknown".into(),
    };
    let status = match d.status {
        Status::Error if !d.error.is_empty() => pill(format!("! Error: {}", d.error), "bad"),
        Status::Error => pill("! Error", "bad"),
        Status::Complete => pill("✓ Complete", "ok"),
        _ => {
            let t = fmt::status(&d);
            pill(
                if t.is_empty() { "Queued".into() } else { t },
                if running { "accent" } else { "muted" },
            )
        }
    };
    let verify = m.verify_progress(id);
    let when = |d: &Download| {
        chrono::DateTime::from_timestamp(d.checksum_checked as i64, 0)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%b %d, %H:%M")
                    .to_string()
            })
            .unwrap_or_default()
    };
    let (cpill, ctext, ctone) = if let Some(v) = verify {
        (
            pill("Verifying", "accent"),
            format!("Checking… {:.0}%", v * 100.0),
            "muted",
        )
    } else {
        match d.checksum_result.as_str() {
            "verified" => (
                pill("✓ Verified", "ok"),
                format!(
                    "Matches · checked {} in {:.1} s",
                    when(&d),
                    d.checksum_seconds
                ),
                "okText",
            ),
            "mismatch" => (
                pill("Checksum mismatch", "bad"),
                format!(
                    "Doesn't match · checked {} in {:.1} s",
                    when(&d),
                    d.checksum_seconds
                ),
                "badText",
            ),
            "skipped" => (
                pill("Not checked", "muted"),
                "Skipped — the file was not checked".into(),
                "muted",
            ),
            _ if !d.checksum.is_empty() => (
                pill("Not checked", "muted"),
                "Not checked yet".into(),
                "muted",
            ),
            _ => (
                pill("Not set", "muted"),
                "No checksum entered".into(),
                "muted",
            ),
        }
    };
    let last = if d.last_try > 0.0 {
        fmt::date(d.last_try)
    } else {
        "never".into()
    };
    let result = if d.status == Status::Error {
        format!("Error: {}", d.error)
    } else {
        fmt::status(&d)
    };
    let tail = match d.checksum_result.as_str() {
        r @ ("verified" | "mismatch") => format!(" · {} {r}", checksum::title(&d.checksum)),
        _ => String::new(),
    };
    Some(json!({
        "download": d,
        "icon": view::category_icon(&d.category),
        "status": status,
        "type": format!("{kind} file ({})", d.category),
        "size": if d.size < 0 { "(unknown)".into() } else { size_with_bytes(d.size) },
        "variant": d.media.as_ref().and_then(|md| md.get("variant")).and_then(|v| v.get("label")).and_then(Value::as_str),
        "youtube": d.media.is_some(),
        "checksum_pill": cpill,
        "checksum_text": ctext,
        "checksum_tone": ctone,
        "verifying": verify.is_some(),
        "complete": d.status == Status::Complete,
        "running": running,
        "footer": format!("Last try: {last} · Result: {result}{tail}"),
    }))
}

/// "Verify now": store a changed checksum, then check the file.
#[tauri::command]
pub fn properties_verify(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    checksum: String,
) -> bool {
    let mut m = state.manager();
    if m.downloads.get(&id).is_some_and(|d| d.checksum != checksum) {
        let cs = checksum.clone();
        m.edit(id, |d| {
            d.checksum = cs.clone();
            d.checksum_result.clear();
            d.checksum_actual.clear();
        });
    }
    let ok = m.verify(id, true);
    drop(m);
    push(&app, &state);
    ok
}

#[derive(Deserialize)]
pub struct PropertiesPatch {
    filename: String,
    save_dir: String,
    url: String,
    description: String,
    referer: String,
    username: String,
    password: String,
    checksum: String,
}

/// OK in File Properties: rename or move (finished files are moved on disk), then the other fields.
#[tauri::command]
pub fn properties_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    patch: PropertiesPatch,
) -> Res<()> {
    let d = state.manager().get(id).ok_or("no such download")?;
    let name = if patch.filename.trim().is_empty() {
        d.filename.clone()
    } else {
        patch.filename.trim().to_string()
    };
    let folder = if patch.save_dir.trim().is_empty() {
        d.save_dir.clone()
    } else {
        patch.save_dir.trim().to_string()
    };
    if name != d.filename || folder != d.save_dir {
        if state.manager().is_running(id) {
            return Err("Stop the download before moving or renaming it.".into());
        }
        if d.status == Status::Complete && std::path::Path::new(&d.path()).exists() {
            let target = std::path::Path::new(&folder).join(&name);
            std::fs::create_dir_all(&folder)
                .and_then(|_| match std::fs::rename(d.path(), &target) {
                    Err(e) if e.raw_os_error() == Some(18) => std::fs::copy(d.path(), &target)
                        .and_then(|_| std::fs::remove_file(d.path())),
                    r => r,
                })
                .map_err(|e| format!("Cannot move the file:\n{e}"))?;
        }
    }
    let changed_checksum = patch.checksum != d.checksum;
    state.manager().edit(id, |x| {
        x.filename = name.clone();
        x.save_dir = folder.clone();
        if !patch.url.trim().is_empty() {
            x.url = patch.url.trim().to_string();
        }
        x.description = patch.description.clone();
        x.referer = patch.referer.clone();
        x.username = patch.username.clone();
        x.password = patch.password.clone();
        if changed_checksum {
            x.checksum = patch.checksum.clone();
            x.checksum_result.clear();
            x.checksum_actual.clear();
            x.checksum_retried = false;
        }
    });
    push(&app, &state);
    Ok(())
}

// ------------------------------------------------------------------ options

#[tauri::command]
pub fn desktop_state(state: State<'_, AppState>) -> Value {
    let s = &state.sys;
    let connected = lider_sys::browser::BROWSERS
        .iter()
        .filter(|b| lider_sys::browser::is_installed(b, s))
        .count();
    json!({
        "autostart": lider_sys::desktop::autostart_enabled(s),
        "in_menu": lider_sys::desktop::in_menu(s),
        "system_installed": lider_sys::desktop::system_installed(s),
        "browsers_connected": connected,
        "global_menu": state.ui().global_menu,
        "default_file_types": lider_core::settings::DEFAULT_FILE_TYPES,
    })
}

/// OK in Options: the edited settings, plus the desktop entries (autostart, application menu).
#[tauri::command]
pub fn options_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Map<String, Value>,
    menu: Option<bool>,
) -> Res<()> {
    let launch = settings
        .get("launch_on_startup")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    settings_patch(app.clone(), state.clone(), settings)?;
    let s = &state.sys;
    if launch != lider_sys::desktop::autostart_enabled(s) {
        // 1.x ignores an OSError here: OK still closes the window.
        let _ = lider_sys::desktop::set_autostart(s, launch);
    }
    if let Some(on) = menu
        && !lider_sys::desktop::system_installed(s)
    {
        let in_menu = lider_sys::desktop::in_menu(s);
        if on && !in_menu {
            let _ = lider_sys::desktop::add_to_menu(s);
        } else if !on && in_menu {
            lider_sys::desktop::remove_from_menu(s);
        }
    }
    state.ui().browser_ok = None;
    push(&app, &state);
    Ok(())
}

// ------------------------------------------------------------------ scheduler

#[tauri::command]
pub fn scheduler_data(state: State<'_, AppState>, queue: String) -> Value {
    let m = state.manager();
    let queues: Vec<Value> = view::queue_names(&m)
        .iter()
        .map(|q| {
            json!({"name": q, "icon": if q == SYNC_QUEUE { "queue-sync" } else { "queue" },
                "count": m.queue_items(q).len(), "running": m.queues.get(q).is_some_and(|x| x.running)})
        })
        .collect();
    let q = m.queues.get(&queue);
    let items: Vec<Value> = m
        .queue_items(&queue)
        .iter()
        .map(|d| {
            let s = fmt::status(d);
            json!({"id": d.id, "name": d.filename, "icon": view::category_icon(&d.category),
                "size": fmt::size(d.size), "status": if s.is_empty() { "Queued".into() } else { s }})
        })
        .collect();
    json!({
        "queues": queues,
        "schedule": q.map(|q| serde_json::to_value(&q.schedule).unwrap_or_default()),
        "running": q.is_some_and(|q| q.running),
        "items": items,
        "today": chrono::Local::now().format("%Y-%m-%d").to_string(),
    })
}

#[tauri::command]
pub fn scheduler_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    queue: String,
    schedule: QueueSchedule,
) {
    {
        let mut m = state.manager();
        if let Some(q) = m.queues.get_mut(&queue) {
            q.schedule = schedule;
        }
        m.save_queue(&queue);
    }
    push(&app, &state);
}

#[tauri::command]
pub fn next_run(schedule: QueueSchedule) -> Option<String> {
    lider_core::queue::next_run_text(&schedule, chrono::Local::now().naive_local())
}

#[tauri::command]
pub fn queue_new(app: AppHandle, state: State<'_, AppState>, name: String) -> bool {
    let name = name.trim().to_string();
    let ok = !name.is_empty() && !state.manager().queues.contains_key(&name);
    if ok {
        state.manager().add_queue(&name);
        push(&app, &state);
    }
    ok
}

/// Delete a queue; the two built-in ones cannot be deleted (Err with the message).
#[tauri::command]
pub fn queue_delete(app: AppHandle, state: State<'_, AppState>, name: String) -> Res<()> {
    if name == MAIN_QUEUE || name == SYNC_QUEUE {
        return Err("The default queues cannot be deleted.".into());
    }
    state.manager().delete_queue(&name);
    push(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn queue_move(app: AppHandle, state: State<'_, AppState>, id: i64, delta: i64) {
    state.manager().move_in_queue(id, delta);
    push(&app, &state);
}

// ------------------------------------------------------------------ browser integration

#[tauri::command]
pub fn browsers_list(state: State<'_, AppState>) -> Value {
    let s = &state.sys;
    let detected = lider_sys::browser::detected(s);
    let rows: Vec<Value> = lider_sys::browser::BROWSERS
        .iter()
        .map(|b| {
            let found = detected.iter().any(|d| d.name == b.name);
            let installed = lider_sys::browser::is_installed(b, s);
            json!({"name": b.name, "family": b.family.as_str(), "detected": found,
                "state": if installed { "installed" } else if found { "not installed" } else { "browser not found" }})
        })
        .collect();
    let folder = |f: lider_sys::browser::Family| {
        let p = s.dirs().data.join("browser-extension").join(f.as_str());
        if p.join("manifest.json").is_file() {
            p.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    };
    json!({
        "rows": rows,
        "chromium_folder": folder(lider_sys::browser::Family::Chromium),
        "firefox_folder": folder(lider_sys::browser::Family::Firefox),
        "chrome_store": lider_sys::browser::CHROME_WEB_STORE_URL,
        "firefox_addons": lider_sys::browser::FIREFOX_ADDONS_URL,
        "firefox_signed": lider_sys::browser::FIREFOX_SIGNED_XPI,
        "firefox_xpi": lider_sys::browser::FIREFOX_XPI_URL,
        "firefox_zip": lider_sys::browser::FIREFOX_ZIP_URL,
        "releases": lider_sys::browser::RELEASES_URL,
    })
}

/// Install or remove the helper for the named browsers; installing exports the extension folders.
#[tauri::command]
pub fn browsers_apply(state: State<'_, AppState>, names: Vec<String>, install: bool) -> Res<()> {
    let s = &state.sys;
    let chosen: Vec<lider_sys::browser::Browser> = lider_sys::browser::BROWSERS
        .iter()
        .filter(|b| names.iter().any(|n| n == b.name))
        .copied()
        .collect();
    if install {
        lider_sys::browser::install(&chosen, s).map_err(|e| e.to_string())?;
        let mut families: Vec<_> = chosen.iter().map(|b| b.family).collect();
        families.dedup();
        for f in families {
            lider_sys::browser::extension_folder(f, s).map_err(|e| e.to_string())?;
        }
    } else {
        lider_sys::browser::uninstall(&chosen, s);
    }
    state.ui().browser_ok = None;
    Ok(())
}

/// Open a web page or file with the desktop's default program.
#[tauri::command]
pub fn open_url(url: String) -> Res<()> {
    crate::system::open_path(&url).map_err(|e| e.to_string())
}

/// A download as JSON (for dialogs that only read it).
#[tauri::command]
pub fn download_get(state: State<'_, AppState>, id: i64) -> Option<Download> {
    state.manager().get(id)
}

#[tauri::command]
pub fn can_check_hash(state: State<'_, AppState>, id: i64) -> bool {
    let m = state.manager();
    m.downloads.get(&id).is_some_and(|d| can_check(&m, d))
}

/// The user's home folder (a new category's folder is `$HOME/Downloads/New`).
#[tauri::command]
pub fn home_dir() -> String {
    lider_core::paths::home_dir().to_string_lossy().into_owned()
}

/// The AppImage's file name, for "… runs on its own".
#[tauri::command]
pub fn appimage_name(state: State<'_, AppState>) -> Option<String> {
    state.sys.appimage.as_ref().and_then(|p| {
        std::path::Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    })
}
