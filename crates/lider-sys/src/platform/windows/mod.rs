//! The Windows adapter: a named pipe for the single instance, registry keys for the browsers' native
//! messaging hosts and for launching at login, Explorer for opening files and folders, `shutdown` for
//! powering off and the registry for the light/dark choice and the fonts.
//!
//! LID is installed per user by its installer, which also adds the Start-menu entry, so the menu part of
//! the `Desktop` port has nothing to do here.

pub mod browsers;
pub mod ipc;

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use lider_core::paths::Dirs;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::browser::Family;

use super::{BrowserHosts, BrowserState, Desktop, Instance, InstanceGuard, Platform, Shell};

/// `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`: no console shared with LID or the browser.
const DETACHED: u32 = 0x0000_0008 | 0x0000_0200;
const RUN_VALUE: &str = "LID";

#[derive(Debug, Clone)]
pub struct Windows {
    pub dirs: Dirs,
    /// `%LOCALAPPDATA%` and `%APPDATA%`, where the browsers keep their profiles.
    pub local_appdata: PathBuf,
    pub appdata: PathBuf,
    /// `Software` under HKCU; tests use a key of their own.
    pub registry_root: String,
    pub current_exe: PathBuf,
}

impl Windows {
    pub fn dirs(&self) -> Dirs {
        self.dirs.clone()
    }

    /// Everything under `root`, and registry keys under `HKCU\<registry_root>`.
    pub fn for_test(root: &Path, registry_root: &str) -> Windows {
        Windows {
            dirs: Dirs::from_parts(
                root.into(),
                Some(root.join("Roaming").into()),
                Some(root.join("Local").into()),
                Some(root.join("Local").into()),
            ),
            local_appdata: root.join("Local"),
            appdata: root.join("Roaming"),
            registry_root: registry_root.into(),
            current_exe: root.join(r"Programs\LID\lider.exe"),
        }
    }

    fn run_key(&self) -> String {
        format!(
            r"{}\Microsoft\Windows\CurrentVersion\Run",
            self.registry_root
        )
    }

    fn autostart_command(&self) -> String {
        format!("\"{}\" --minimized", self.current_exe.display())
    }
}

impl Platform for Windows {
    fn from_process() -> Windows {
        let home = lider_core::paths::home_dir();
        let var = |name: &str, default: PathBuf| {
            std::env::var_os(name).map(PathBuf::from).unwrap_or(default)
        };
        Windows {
            dirs: Dirs::from_env(),
            local_appdata: var("LOCALAPPDATA", home.join(r"AppData\Local")),
            appdata: var("APPDATA", home.join(r"AppData\Roaming")),
            registry_root: "Software".into(),
            current_exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("lider.exe")),
        }
    }
}

impl Instance for Windows {
    fn instance_name(&self) -> String {
        ipc::pipe_name()
    }
    fn bind(
        &self,
        name: &str,
        on_line: Box<dyn Fn(String) + Send + Sync>,
    ) -> io::Result<InstanceGuard> {
        ipc::InstanceServer::bind(name, on_line).map(InstanceGuard::new)
    }
    fn send_lines(&self, lines: &[String], name: &str) -> bool {
        ipc::send_lines(lines, name)
    }
    fn is_running(&self, name: &str) -> bool {
        ipc::is_running(name)
    }
}

impl Desktop for Windows {
    /// The login entry follows LID when it was moved or updated in place.
    fn refresh(&self) -> Vec<&'static str> {
        if self.autostart_enabled() {
            let current = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(self.run_key())
                .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
                .unwrap_or_default();
            if current != self.autostart_command() && self.set_autostart(true).is_ok() {
                return vec!["autostart"];
            }
        }
        vec![]
    }
    fn should_offer(&self, _menu_offered: bool) -> bool {
        false
    }
    /// The installer adds the Start-menu entry.
    fn system_installed(&self) -> bool {
        true
    }
    fn in_menu(&self) -> bool {
        true
    }
    fn add_to_menu(&self) -> io::Result<()> {
        Ok(())
    }
    fn remove_from_menu(&self) {}
    fn autostart_enabled(&self) -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(self.run_key())
            .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
            .is_ok()
    }
    fn set_autostart(&self, on: bool) -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(self.run_key())?;
        if on {
            key.set_value(RUN_VALUE, &self.autostart_command())
        } else {
            match key.delete_value(RUN_VALUE) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }
    fn self_contained_name(&self) -> Option<String> {
        None
    }
}

impl BrowserHosts for Windows {
    fn browsers(&self) -> Vec<BrowserState> {
        self.states()
    }
    fn install(&self, names: &[&str]) -> io::Result<PathBuf> {
        self.install_hosts(names)
    }
    fn uninstall(&self, names: &[&str]) {
        self.uninstall_hosts(names)
    }
    fn extension_folder(&self, family: Family) -> io::Result<PathBuf> {
        self.export(family)
    }
    fn exported_extension(&self, family: Family) -> Option<PathBuf> {
        let p = self.extension_dir(family);
        p.join("manifest.json").is_file().then_some(p)
    }
}

/// A command line as Windows writes it: the program, quoted when it has spaces, then its arguments.
pub fn split_command(command: &str) -> Option<Vec<String>> {
    let s = command.trim();
    if s.is_empty() {
        return None;
    }
    let (program, rest) = if let Some(r) = s.strip_prefix('"') {
        let end = r.find('"')?;
        (r[..end].to_string(), &r[end + 1..])
    } else {
        let end = s.find(char::is_whitespace).unwrap_or(s.len());
        (s[..end].to_string(), &s[end..])
    };
    let mut out = vec![program];
    out.extend(rest.split_whitespace().map(String::from));
    Some(out)
}

impl Shell for Windows {
    /// Explorer opens a file with its program, a folder in a window and a web address in the browser.
    fn open_path(&self, path: &str) -> io::Result<()> {
        self.spawn_detached("explorer.exe", &[path.to_string()])
    }
    fn open_folder(&self, path: &str) -> io::Result<()> {
        if Path::new(path).exists() {
            self.spawn_detached("explorer.exe", &[format!("/select,{path}")])
        } else {
            let parent = Path::new(path)
                .parent()
                .map_or(path.to_string(), |p| p.to_string_lossy().into_owned());
            self.open_path(&parent)
        }
    }
    fn open_with(&self, command: &str, path: &str) -> io::Result<()> {
        let mut args =
            split_command(command).ok_or_else(|| io::Error::other("Not a valid command"))?;
        let program = args.remove(0);
        args.push(path.to_string());
        self.spawn_detached(&program, &args)
    }
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()> {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED)
            .spawn()
            .map(|_| ())
    }
    fn power_off(&self) -> io::Result<()> {
        self.spawn_detached("shutdown.exe", &["/s".into(), "/t".into(), "0".into()])
    }
    /// `AppsUseLightTheme` (0 = dark) in the personalisation settings.
    fn prefers_dark(&self) -> Option<bool> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .ok()?;
        key.get_value::<u32, _>("AppsUseLightTheme")
            .ok()
            .map(|v| v == 0)
    }
    fn global_menu_available(&self) -> bool {
        false
    }
    /// The families of the installed fonts (machine-wide and the user's own), from their registry names
    /// such as "Segoe UI Semibold Italic (TrueType)".
    fn font_families(&self) -> Vec<String> {
        const STYLES: [&str; 14] = [
            "Bold",
            "Italic",
            "Oblique",
            "Light",
            "Semibold",
            "SemiBold",
            "Semilight",
            "Black",
            "Thin",
            "Medium",
            "ExtraBold",
            "ExtraLight",
            "Regular",
            "Condensed",
        ];
        let mut v = vec![];
        for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
            let Ok(key) = RegKey::predef(root)
                .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts")
            else {
                continue;
            };
            for (name, _) in key.enum_values().flatten() {
                let name = name.split(" (").next().unwrap_or(&name);
                for one in name.split(" & ") {
                    let words: Vec<&str> = one
                        .split_whitespace()
                        .filter(|w| !STYLES.contains(w))
                        .collect();
                    if !words.is_empty() {
                        v.push(words.join(" "));
                    }
                }
            }
        }
        v.sort_by_key(|s| s.to_lowercase());
        v.dedup();
        v
    }
    fn mono_family(&self) -> String {
        "Consolas".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_split_like_windows() {
        assert_eq!(
            split_command(r#""C:\Program Files\VLC\vlc.exe" --fullscreen"#).unwrap(),
            [r"C:\Program Files\VLC\vlc.exe", "--fullscreen"]
        );
        assert_eq!(split_command(r"notepad.exe").unwrap(), ["notepad.exe"]);
        assert!(split_command("  ").is_none());
    }

    #[test]
    fn autostart_on_and_off() {
        let tmp = tempfile::tempdir().unwrap();
        let os = Windows::for_test(
            tmp.path(),
            &format!(r"Software\lider-test-run-{}", std::process::id()),
        );
        assert!(!os.autostart_enabled());
        os.set_autostart(true).unwrap();
        assert!(os.autostart_enabled());
        os.set_autostart(false).unwrap();
        assert!(!os.autostart_enabled());
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&os.registry_root);
    }
}
