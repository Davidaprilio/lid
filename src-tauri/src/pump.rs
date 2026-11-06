//! The 500 ms tick of 1.x `MainWindow.tick`: poll the manager, react to its events, watch the clipboard
//! and the desktop's colour scheme, and send the window what it shows.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::app::{self, AppState};

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("lider-tick".into())
        .spawn(move || {
            let mut clipboard = arboard::Clipboard::new().ok();
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let state = app.state::<AppState>();
                tick(&app, state.inner(), clipboard.as_mut());
            }
        })
        .expect("the tick thread starts");
}

fn tick(app: &AppHandle, state: &AppState, clipboard: Option<&mut arboard::Clipboard>) {
    let events = state.manager().poll(None);
    app::handle_events(app, state, &events);
    let n = {
        let mut ui = state.ui();
        ui.tick += 1;
        ui.tick
    };
    if n % 4 == 1 {
        let dark = crate::system::prefers_dark().unwrap_or(false);
        state.ui().dark_system = dark;
    }
    if n % 20 == 1 {
        let global = crate::system::global_menu_available();
        let changed = {
            let mut ui = state.ui();
            let c = ui.global_menu != global;
            ui.global_menu = global;
            c
        };
        if changed {
            apply_menubar(app, state);
        }
    }
    if let Some(c) = clipboard
        && let Ok(text) = c.get_text()
    {
        app::clipboard_text(app, state, &text);
    }
    push_view(app, state);
    let tip = lider_core::view::tray_tooltip(&state.manager());
    crate::tray::set_tooltip(app, &tip);
}

/// Send the main window what it shows now (after a change, or each tick).
pub fn push_view(app: &AppHandle, state: &AppState) {
    if !*app::lock(&state.ready) {
        return;
    }
    let view = app::main_view(state, false);
    if view.menus.is_some() {
        set_native_menu(app, state);
    }
    let _ = app.emit_to(
        "main",
        "lid://view",
        serde_json::to_value(&view).unwrap_or_default(),
    );
}

fn native_mode(state: &AppState) -> bool {
    state.manager().settings.menubar != "custom"
}

fn set_native_menu(app: &AppHandle, state: &AppState) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    if !native_mode(state) {
        let _ = w.remove_menu();
        return;
    }
    let menus = {
        let m = state.manager();
        let ui = state.ui();
        let targets = lider_core::view::targets(&ui.visible, &ui.checked, &ui.selected);
        let checked = ui.visible.iter().filter(|i| ui.checked.contains(i)).count();
        crate::menu::menus(&m, &lider_core::view::actions(&m, &targets, checked))
    };
    if let Ok(menu) = crate::menu::native(app, &menus) {
        let _ = w.set_menu(menu);
    }
}

/// Native, in the window, or both (1.x `set_menubar_mode`).
pub fn apply_menubar(app: &AppHandle, state: &AppState) {
    set_native_menu(app, state);
    push_view(app, state);
}
