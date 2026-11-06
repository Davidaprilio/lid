//! Desktop integration for a copy of LID that no package installed (an AppImage or a loose binary):
//! the application-menu entry, the autostart entry, and keeping them, and the browser helper, pointed
//! at the copy that runs now, so they keep working after an AppImage is moved or replaced
//! (`desktop.py` in 1.x).

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use crate::browser::{host_executable, wrapper_script};
use crate::env::{Launch, SysEnv};

/// The app icon installed with the menu entry.
pub const ICON: &[u8] = include_bytes!("../../../lider/ui/assets/icons/app.svg");
/// Entries LID wrote, which it may rewrite or remove.
const MARK: &str = "X-LID-Written-By=LID";
const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";

pub fn menu_entry_path(env: &SysEnv) -> PathBuf {
    env.data_home.join("applications/lider.desktop")
}

pub fn autostart_path(env: &SysEnv) -> PathBuf {
    env.config_home.join("autostart/lider.desktop")
}

pub fn icon_path(env: &SysEnv) -> PathBuf {
    env.data_home.join("icons/hicolor/scalable/apps/lider.svg")
}

/// A desktop entry value of type string.
fn string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

/// One argument of a desktop entry's Exec key (quoting and escaping of the Desktop Entry spec).
pub fn exec_arg(arg: &str) -> String {
    let arg = arg.replace('%', "%%");
    if !arg.is_empty() && !arg.chars().any(|c| RESERVED.contains(c)) {
        return arg;
    }
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        if "\"`$\\".contains(c) {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    string(&quoted) // the key's value is a string: its backslashes are escaped again
}

fn exec(env: &SysEnv, extra: &[&str]) -> String {
    std::iter::once(env.launch_program())
        .chain(extra.iter().map(|s| s.to_string()))
        .map(|a| exec_arg(&a))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn menu_entry_text(env: &SysEnv) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=LID\n\
         GenericName=Download Manager\n\
         Comment=Linux Internet Downloader: accelerated multi-connection downloads\n\
         Exec={} %U\n\
         TryExec={}\n\
         Icon=lider\n\
         Terminal=false\n\
         Categories=Network;FileTransfer;\n\
         MimeType=x-scheme-handler/http;x-scheme-handler/https;x-scheme-handler/ftp;\n\
         {MARK}\n",
        exec(env, &[]),
        string(&env.launch_program()), // a plain path; hides the entry once the AppImage is gone
    )
}

pub fn autostart_text(env: &SysEnv) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=LID\n\
         Exec={}\n\
         Icon=lider\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exec(env, &["--minimized"])
    )
}

/// A package put LID in the application menu already.
pub fn system_installed(env: &SysEnv) -> bool {
    env.system_entry.exists()
}

pub fn in_menu(env: &SysEnv) -> bool {
    menu_entry_path(env).is_file()
}

/// Another tool (AppImageLauncher, Gear Lever, appimaged) already gave this AppImage a menu entry.
pub fn listed_elsewhere(env: &SysEnv) -> bool {
    let Some(appimage) = &env.appimage else {
        return false;
    };
    let ours = menu_entry_path(env);
    let Ok(entries) = fs::read_dir(ours.parent().unwrap()) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        p.extension().is_some_and(|x| x == "desktop")
            && p.file_name() != ours.file_name()
            && fs::read(&p).is_ok_and(|t| String::from_utf8_lossy(&t).contains(appimage.as_str()))
    })
}

/// The first-start question "Add LID to the application menu?" is for an AppImage that no menu lists.
pub fn should_offer(env: &SysEnv, menu_offered: bool) -> bool {
    env.appimage.is_some()
        && !(menu_offered || in_menu(env) || system_installed(env) || listed_elsewhere(env))
}

pub fn add_to_menu(env: &SysEnv) -> io::Result<()> {
    let icon = icon_path(env);
    fs::create_dir_all(icon.parent().unwrap())?;
    fs::write(&icon, ICON)?;
    let entry = menu_entry_path(env);
    fs::create_dir_all(entry.parent().unwrap())?;
    fs::write(&entry, menu_entry_text(env))?;
    update_menus(env);
    Ok(())
}

pub fn remove_from_menu(env: &SysEnv) {
    let _ = fs::remove_file(menu_entry_path(env));
    let _ = fs::remove_file(icon_path(env));
    update_menus(env);
}

/// Desktops watch the folder; the cache of x-scheme-handler entries needs this nudge.
fn update_menus(env: &SysEnv) {
    let folder = menu_entry_path(env).parent().unwrap().to_path_buf();
    if env.update_menus && folder.is_dir() {
        let _ = Command::new("update-desktop-database")
            .arg(&folder)
            .output();
    }
}

pub fn autostart_enabled(env: &SysEnv) -> bool {
    autostart_path(env).is_file()
}

pub fn set_autostart(env: &SysEnv, on: bool) -> io::Result<()> {
    let path = autostart_path(env);
    if on {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, autostart_text(env))
    } else {
        match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Point the files LID wrote at the copy that runs now. Returns what was rewritten.
pub fn refresh(env: &SysEnv) -> Vec<&'static str> {
    if matches!(env.launch(), Launch::Checkout(_)) {
        return vec![]; // a source build: the entries belong to the installed copy
    }
    let mut done = Vec::new();
    let mut files: Vec<(&'static str, PathBuf, String)> = vec![
        ("autostart", autostart_path(env), autostart_text(env)),
        ("browser helper", host_executable(env), wrapper_script(env)),
    ];
    // An entry without the mark is the user's (a menu editor saves its changes to the same file).
    let entry = menu_entry_path(env);
    if fs::read_to_string(&entry).is_ok_and(|t| t.contains(MARK)) {
        if system_installed(env) {
            remove_from_menu(env); // the package's own entry takes over
            done.push("menu");
        } else {
            files.insert(0, ("menu", entry, menu_entry_text(env)));
        }
    }
    for (name, path, wanted) in files {
        if let Ok(have) = fs::read_to_string(&path)
            && have != wanted
            && fs::write(&path, wanted).is_ok()
        {
            done.push(name);
        }
    }
    done
}

#[cfg(test)]
mod tests {
    //! Port of tests/test_desktop.py (the Options-dialog case comes with the UI).
    use super::*;
    use crate::browser;
    use std::path::Path;

    const APPIMAGE: &str = "/opt/apps/Lider 1.2.0.AppImage";

    fn env(root: &Path) -> SysEnv {
        SysEnv::for_test(root, Some(APPIMAGE))
    }

    #[test]
    fn exec_quoting() {
        assert_eq!(exec_arg("/usr/bin/lider"), "/usr/bin/lider");
        assert_eq!(exec_arg("/a b/LID"), "\"/a b/LID\"");
        assert_eq!(exec_arg("/a/$x"), "\"/a/\\\\$x\"");
        assert_eq!(exec_arg("/a/\"b\""), "\"/a/\\\\\"b\\\\\"\"");
        assert_eq!(exec_arg("/a/100%"), "/a/100%%");
    }

    #[test]
    fn menu_entry_runs_the_appimage() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        let text = menu_entry_text(&e);
        assert!(text.contains(&format!("Exec=\"{APPIMAGE}\" %U\n")));
        assert!(text.contains(&format!("TryExec={APPIMAGE}\n")));
        assert!(text.contains("Icon=lider\n"));
        assert!(autostart_text(&e).contains(&format!("Exec=\"{APPIMAGE}\" --minimized\n")));
    }

    #[test]
    fn entry_passes_desktop_file_validate() {
        let Ok(out) = Command::new("desktop-file-validate")
            .arg("--version")
            .output()
        else {
            eprintln!("skipped: desktop-file-validate is not installed");
            return;
        };
        let _ = out;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lider.desktop");
        fs::write(&path, menu_entry_text(&env(tmp.path()))).unwrap();
        let out = Command::new("desktop-file-validate")
            .arg(&path)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        assert!(!text.contains("error"), "{text}");
    }

    #[test]
    fn add_and_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        assert!(!in_menu(&e));
        add_to_menu(&e).unwrap();
        assert!(in_menu(&e));
        assert_eq!(fs::read(icon_path(&e)).unwrap(), ICON);
        assert_eq!(
            menu_entry_path(&e),
            tmp.path().join("data/applications/lider.desktop")
        );
        remove_from_menu(&e);
        assert!(!in_menu(&e));
        assert!(!icon_path(&e).exists());
    }

    #[test]
    fn autostart() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        set_autostart(&e, true).unwrap();
        assert!(autostart_enabled(&e));
        assert_eq!(
            autostart_path(&e),
            tmp.path().join("config/autostart/lider.desktop")
        );
        set_autostart(&e, false).unwrap();
        assert!(!autostart_enabled(&e));
        set_autostart(&e, false).unwrap();
    }

    #[test]
    fn refresh_follows_a_moved_appimage() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = env(tmp.path());
        add_to_menu(&e).unwrap();
        set_autostart(&e, true).unwrap();
        browser::install(&[], &e).unwrap();
        assert!(refresh(&e).is_empty());
        e.appimage = Some("/home/me/Apps/Lider-1.3.0-x86_64.AppImage".into());
        assert_eq!(refresh(&e), ["menu", "autostart", "browser helper"]);
        assert!(
            fs::read_to_string(menu_entry_path(&e))
                .unwrap()
                .contains("Exec=/home/me/Apps/Lider-1.3.0-x86_64.AppImage %U")
        );
        assert!(
            fs::read_to_string(browser::host_executable(&e))
                .unwrap()
                .contains("exec /home/me/Apps/Lider-1.3.0-x86_64.AppImage --native-host")
        );
        assert!(!listed_elsewhere(&e));
    }

    #[test]
    fn refresh_leaves_missing_files_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        assert!(refresh(&e).is_empty());
        assert!(!in_menu(&e));
        assert!(!autostart_enabled(&e));
    }

    #[test]
    fn a_checkout_does_not_take_over_the_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = env(tmp.path());
        add_to_menu(&e).unwrap();
        let before = fs::read_to_string(menu_entry_path(&e)).unwrap();
        e.appimage = None;
        e.installed = None;
        e.current_exe = PathBuf::from("/home/me/src/lid/target/debug/lider");
        assert!(matches!(e.launch(), Launch::Checkout(_)));
        assert!(refresh(&e).is_empty());
        assert_eq!(fs::read_to_string(menu_entry_path(&e)).unwrap(), before);
    }

    #[test]
    fn an_entry_from_a_menu_editor_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        fs::create_dir_all(menu_entry_path(&e).parent().unwrap()).unwrap();
        let mine = "[Desktop Entry]\nType=Application\nName=My downloader\nExec=lider %U\n";
        fs::write(menu_entry_path(&e), mine).unwrap();
        assert!(refresh(&e).is_empty());
        assert_eq!(fs::read_to_string(menu_entry_path(&e)).unwrap(), mine);
    }

    #[test]
    fn an_old_autostart_entry_is_fixed() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        fs::create_dir_all(autostart_path(&e).parent().unwrap()).unwrap();
        fs::write(
            autostart_path(&e),
            "[Desktop Entry]\nType=Application\nName=LID\nExec=python3 -m lider --minimized\nX-GNOME-Autostart-enabled=true\n",
        )
        .unwrap();
        assert_eq!(refresh(&e), ["autostart"]);
        assert!(
            fs::read_to_string(autostart_path(&e))
                .unwrap()
                .contains(&format!("Exec=\"{APPIMAGE}\" --minimized"))
        );
    }

    #[test]
    fn a_1x_menu_entry_is_updated_on_the_first_2x_start() {
        // 1.x wrote "Categories=Network;FileTransfer;Qt;"; the marked entry is rewritten.
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        fs::create_dir_all(menu_entry_path(&e).parent().unwrap()).unwrap();
        let old = menu_entry_text(&e).replace("FileTransfer;\n", "FileTransfer;Qt;\n");
        fs::write(menu_entry_path(&e), old).unwrap();
        assert_eq!(refresh(&e), ["menu"]);
        assert!(
            !fs::read_to_string(menu_entry_path(&e))
                .unwrap()
                .contains("Qt;")
        );
    }

    #[test]
    fn package_entry_takes_over() {
        let tmp = tempfile::tempdir().unwrap();
        let e = env(tmp.path());
        add_to_menu(&e).unwrap();
        fs::create_dir_all(e.system_entry.parent().unwrap()).unwrap();
        fs::write(&e.system_entry, "[Desktop Entry]\n").unwrap();
        assert!(refresh(&e).contains(&"menu"));
        assert!(!in_menu(&e));
    }

    #[test]
    fn offer_once_and_only_to_an_appimage() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = env(tmp.path());
        assert!(should_offer(&e, false));
        assert!(!should_offer(&e, true));
        let folder = menu_entry_path(&e).parent().unwrap().to_path_buf();
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("appimagekit_lider.desktop"),
            format!("[Desktop Entry]\nExec={APPIMAGE}\n"),
        )
        .unwrap();
        assert!(listed_elsewhere(&e));
        assert!(!should_offer(&e, false));
        e.appimage = None;
        assert!(!should_offer(&e, false));
    }
}
