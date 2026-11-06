//! Desktop services: open files and folders, power off, the system's light/dark choice, the global menu,
//! the font list (`ui/util.py`, `theme.py` and parts of `main_window.py` in 1.x).

use std::process::Command;

use zbus::blocking::Connection;

pub use lider_sys::sh::spawn_detached;

/// Open a file or folder with the desktop's default program.
pub fn open_path(path: &str) -> std::io::Result<()> {
    spawn_detached("xdg-open", &[path.to_string()])
}

/// Show the file selected in the file manager (FileManager1.ShowItems); the folder when that fails.
pub fn open_folder(path: &str) -> std::io::Result<()> {
    let shown = tauri::Url::from_file_path(path).ok().is_some_and(|uri| {
        Connection::session()
            .and_then(|c| {
                c.call_method(
                    Some("org.freedesktop.FileManager1"),
                    "/org/freedesktop/FileManager1",
                    Some("org.freedesktop.FileManager1"),
                    "ShowItems",
                    &(vec![uri.to_string()], ""),
                )
            })
            .is_ok()
    });
    if shown {
        return Ok(());
    }
    let parent = std::path::Path::new(path)
        .parent()
        .map_or(path.to_string(), |p| p.to_string_lossy().into_owned());
    open_path(&parent)
}

/// "Open with…": the command split like a shell line, with the file appended.
pub fn open_with(command: &str, path: &str) -> std::io::Result<()> {
    let mut args = lider_sys::sh::split(command)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| std::io::Error::other("Not a valid command"))?;
    let program = args.remove(0);
    args.push(path.to_string());
    spawn_detached(&program, &args)
}

pub fn power_off() -> std::io::Result<()> {
    spawn_detached("systemctl", &["poweroff".into()])
}

/// The desktop's colour scheme from the settings portal: Some(true) dark, Some(false) light, None unknown.
pub fn prefers_dark() -> Option<bool> {
    let conn = Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.Settings"),
            "ReadOne",
            &("org.freedesktop.appearance", "color-scheme"),
        )
        .ok()?;
    let v: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
    let n = u32::try_from(&v).ok()?;
    // 1 = prefer dark, 2 = prefer light, 0 = no preference.
    Some(n == 1)
}

/// A desktop global menu is available (`com.canonical.AppMenu.Registrar` on the session bus).
pub fn global_menu_available() -> bool {
    Connection::session()
        .and_then(|c| {
            c.call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &("com.canonical.AppMenu.Registrar",),
            )
        })
        .ok()
        .and_then(|r| r.body().deserialize::<bool>().ok())
        .unwrap_or(false)
}

/// Font families installed on the system (fontconfig), sorted.
pub fn font_families() -> Vec<String> {
    let Ok(out) = Command::new("fc-list").args([":", "family"]).output() else {
        return vec![];
    };
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

/// The desktop's monospace family ("monospace" resolved by fontconfig).
pub fn mono_family() -> String {
    Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "monospace".into())
}
