//! LID's other windows: one per dialog, one progress window per download (they are top-level windows in
//! 1.x too, and some must appear while the main window sits in the tray). Every window loads the same
//! frontend; the route after `#` picks the page.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn progress_label(id: i64) -> String {
    format!("progress-{id}")
}

pub fn is_open(app: &AppHandle, label: &str) -> bool {
    app.get_webview_window(label).is_some()
}

pub fn close(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
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

/// Open (or bring forward) a window showing `route`.
pub fn open(
    app: &AppHandle,
    label: &str,
    route: &str,
    title: &str,
    size: (f64, f64),
    modal: bool,
) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let mut b = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("index.html#/{route}").into()),
    )
    .title(title)
    .inner_size(size.0, size.1)
    .min_inner_size(size.0.min(360.0), size.1.min(200.0))
    .disable_drag_drop_handler()
    .center();
    if modal && let Some(main) = app.get_webview_window("main") {
        b = b.parent(&main)?;
    }
    b.build()?;
    Ok(())
}
