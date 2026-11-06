//! LID's other windows: one per dialog, one progress window per download (they are top-level windows in
//! 1.x too, and some must appear while the main window sits in the tray). Every window loads the same
//! frontend; the route after `#` picks the page.
//!
//! A new webview takes about a second to start (window, WebKit process, page), where a Qt dialog of 1.x
//! appeared at once. So one hidden *spare* window is kept loaded: a dialog takes it over (title, size,
//! then its route through `lid://route`), and the next spare is made in the background. A window keeps
//! the label it was created with, so the dialog's own label (`options`, `progress-5`) is an alias of the
//! spare's (`spare-3`): every lookup by label goes through [`get`]. Windows stay hidden until their page
//! has its size ([`dialog_ready`]), so they no longer jump.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// A dialog's label → the label of the window that shows it (a former spare).
static ALIAS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);
/// The loaded, hidden spare window, if one is ready (its page listens for `lid://route`).
static SPARE: Mutex<Option<String>> = Mutex::new(None);
/// A spare being made: built, its page not loaded yet.
static MAKING: Mutex<Option<String>> = Mutex::new(None);
static NEXT_SPARE: AtomicU64 = AtomicU64::new(1);

fn aliases() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    ALIAS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The window showing the dialog `label`.
pub fn get(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    let actual = aliases()
        .get(label)
        .cloned()
        .unwrap_or_else(|| label.to_string());
    app.get_webview_window(&actual)
}

/// The dialog label a window shows (its own label unless it was a spare).
pub fn logical(actual: &str) -> String {
    aliases()
        .iter()
        .find(|(_, a)| a.as_str() == actual)
        .map_or_else(|| actual.to_string(), |(l, _)| l.clone())
}

/// A window went away: forget what pointed at it.
pub fn forget(actual: &str) {
    aliases().retain(|_, a| a != actual);
    for slot in [&SPARE, &MAKING] {
        let mut s = slot.lock().unwrap_or_else(|e| e.into_inner());
        if s.as_deref() == Some(actual) {
            *s = None;
        }
    }
}

fn spares_enabled() -> bool {
    std::env::var_os("LIDER_NO_SPARE_WINDOW").is_none()
}

/// Make the next spare window, after `delay` (so it does not compete with the window just opened).
pub fn prepare_spare(app: &AppHandle, delay: Duration) {
    if !spares_enabled() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if SPARE.lock().unwrap_or_else(|e| e.into_inner()).is_some()
            || MAKING.lock().unwrap_or_else(|e| e.into_inner()).is_some()
        {
            return;
        }
        let label = format!("spare-{}", NEXT_SPARE.fetch_add(1, Ordering::Relaxed));
        let built =
            WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html#/spare".into()))
                .title(lider_core::APP_NAME)
                .visible(false)
                .disable_drag_drop_handler()
                .build();
        if built.is_ok() {
            *MAKING.lock().unwrap_or_else(|e| e.into_inner()) = Some(label);
        }
    });
}

/// Keep a dialog above the main window, as `.parent()` does when a window is built (GTK transient).
fn keep_above_main(app: &AppHandle, w: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    if let (Some(main), Ok(gw)) = (app.get_webview_window("main"), w.gtk_window())
        && let Ok(mw) = main.gtk_window()
    {
        use gtk::prelude::GtkWindowExt;
        gw.set_transient_for(Some(&mw));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (app, w);
}

/// Show a window once its page has its size; a page that never says so is shown after a while anyway.
fn show_later(w: &WebviewWindow, after: Duration) {
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(after);
        if !w.is_visible().unwrap_or(true) {
            let _ = w.show();
            let _ = w.set_focus();
        }
    });
}

/// A spare's page has loaded and listens for its route: it can be taken over now.
#[tauri::command]
pub fn spare_ready(window: WebviewWindow) {
    let mut making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    if making.as_deref() == Some(window.label()) {
        *making = None;
        *SPARE.lock().unwrap_or_else(|e| e.into_inner()) = Some(window.label().to_string());
    }
}

/// The page of this window has laid itself out: show it.
#[tauri::command]
pub fn dialog_ready(window: WebviewWindow) {
    if !window.is_visible().unwrap_or(true) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Close the dialog `label` (for the UI scenarios).
#[tauri::command]
pub fn dialog_close(app: AppHandle, label: String) {
    if let Some(w) = get(&app, &label) {
        let _ = w.close();
    }
}

/// A dialog window with this label is open (for the UI scenarios).
#[tauri::command]
pub fn dialog_open(app: AppHandle, label: String) -> bool {
    get(&app, &label).is_some()
}

pub fn progress_label(id: i64) -> String {
    format!("progress-{id}")
}

pub fn is_open(app: &AppHandle, label: &str) -> bool {
    get(app, label).is_some()
}

pub fn close(app: &AppHandle, label: &str) {
    if let Some(w) = get(app, label) {
        let _ = w.destroy();
    }
    lock().remove(label);
}

/// Every window but the main one.
pub fn close_all_dialogs(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label != "main" {
            let _ = w.destroy();
        }
    }
    lock().clear();
}

/// "Options on completion" of a progress window (show the Complete dialog, exit, turn off).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionOptions {
    pub show_complete: bool,
    pub exit: bool,
    pub shutdown: bool,
}

static OPTIONS: LazyLock<Mutex<HashMap<String, CompletionOptions>>> =
    LazyLock::new(Default::default);

fn lock() -> std::sync::MutexGuard<'static, HashMap<String, CompletionOptions>> {
    OPTIONS.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the progress window of `id` asks for when the download completes (None without one).
pub fn completion_options(app: &AppHandle, id: i64) -> Option<CompletionOptions> {
    let label = progress_label(id);
    is_open(app, &label)
        .then(|| lock().get(&label).copied())
        .flatten()
}

pub fn set_completion_options(label: &str, o: CompletionOptions) {
    lock().insert(label.to_string(), o);
}

/// The narrowest a window may get: the progress window keeps 1.x's 600 px, the others their own width up to 360.
fn min_width(label: &str, width: f64) -> f64 {
    if label.starts_with("progress-") {
        width.min(600.0)
    } else {
        width.min(360.0)
    }
}

/// Open (or bring forward) a window showing `route`.
pub fn open(
    app: &AppHandle,
    label: &str,
    route: &str,
    title: &str,
    size: (f64, f64),
    modal: bool,
) -> tauri::Result<()> {
    if let Some(w) = get(app, label) {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let spare = SPARE.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(w) = spare.and_then(|s| app.get_webview_window(&s)) {
        let _ = w.set_title(title);
        let _ = w.set_min_size(Some(tauri::LogicalSize::new(
            min_width(label, size.0),
            size.1.min(200.0),
        )));
        let _ = w.set_size(tauri::LogicalSize::new(size.0, size.1));
        let _ = w.center();
        if modal {
            keep_above_main(app, &w);
        }
        aliases().insert(label.to_string(), w.label().to_string());
        timing_start(w.label());
        let _ = w.emit_to(w.label(), "lid://route", route);
        show_later(&w, Duration::from_millis(1500));
        prepare_spare(app, Duration::from_millis(1500));
        return Ok(());
    }
    let mut b = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("index.html#/{route}").into()),
    )
    .title(title)
    .inner_size(size.0, size.1)
    .min_inner_size(min_width(label, size.0), size.1.min(200.0))
    .disable_drag_drop_handler()
    .visible(false)
    .center();
    if modal && let Some(main) = app.get_webview_window("main") {
        b = b.parent(&main)?;
    }
    let w = b.build()?;
    timing_start(label);
    show_later(&w, Duration::from_millis(3000));
    prepare_spare(app, Duration::from_millis(1500));
    Ok(())
}

static OPENED: LazyLock<Mutex<HashMap<String, std::time::Instant>>> =
    LazyLock::new(Default::default);

fn timing_start(label: &str) {
    if std::env::var_os("LIDER_TIMING").is_some() {
        OPENED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(label.into(), std::time::Instant::now());
    }
}

/// `LIDER_TIMING=1`: how long a dialog window takes from the request to each stage of its page.
#[tauri::command]
pub fn dialog_mark(window: tauri::WebviewWindow, stage: String) {
    if let Some(t) = OPENED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(window.label())
    {
        eprintln!(
            "[timing] {:<18} {:<8} +{} ms",
            window.label(),
            stage,
            t.elapsed().as_millis()
        );
    }
}
