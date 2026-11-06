//! The tray icon (1.x `_build_tray`): "Open LID", "Add new download", the speed limiter, "Exit".

use tauri::menu::{
    CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

use crate::app::{AppState, show_normal};

const ID: &str = "lid-tray";

/// Create or remove the icon to match `settings.tray_icon`; refresh its menu checks.
pub fn ensure(app: &AppHandle, state: &AppState) {
    let (wanted, limiter_on) = {
        let m = state.manager();
        (m.settings.tray_icon, m.settings.speed_limiter_on)
    };
    if !wanted {
        let _ = app.remove_tray_by_id(ID);
        return;
    }
    let menu = (|| {
        let limiter = SubmenuBuilder::new(app, "Speed Limiter")
            .item(
                &CheckMenuItemBuilder::with_id("limiter_on", "Turn on")
                    .checked(limiter_on)
                    .build(app)?,
            )
            .item(
                &CheckMenuItemBuilder::with_id("limiter_off", "Turn off")
                    .checked(!limiter_on)
                    .build(app)?,
            )
            .build()?;
        MenuBuilder::new(app)
            .item(&MenuItemBuilder::with_id("tray_open", "Open LID").build(app)?)
            .item(&MenuItemBuilder::with_id("add", "&Add new download").build(app)?)
            .item(&PredefinedMenuItem::separator(app)?)
            .item(&limiter)
            .item(&PredefinedMenuItem::separator(app)?)
            .item(&MenuItemBuilder::with_id("exit", "&Exit").build(app)?)
            .build()
    })();
    let Ok(menu) = menu else { return };
    if let Some(t) = app.tray_by_id(ID) {
        let _ = t.set_menu(Some(menu));
        return;
    }
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).ok();
    let mut b = TrayIconBuilder::with_id(ID)
        .tooltip("LID")
        .menu(&menu)
        .show_menu_on_left_click(false);
    if let Some(i) = icon {
        b = b.icon(i);
    }
    let _ = b
        .on_menu_event(|app, e| {
            let id = e.id().as_ref().to_string();
            if id == "tray_open" {
                show_normal(app);
            } else {
                crate::commands::run_menu(app, &id);
            }
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_normal(tray.app_handle());
            }
        })
        .build(app);
    let _ = app.emit("lid://tray", true);
}

pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(t) = app.tray_by_id(ID) {
        let _ = t.set_tooltip(Some(text));
    }
}

pub fn exists(app: &AppHandle) -> bool {
    app.tray_by_id(ID).is_some()
}
