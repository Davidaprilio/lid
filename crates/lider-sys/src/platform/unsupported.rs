//! The adapter for an OS without its own yet (Windows, macOS): LID compiles and runs, and every OS
//! service reports that it is not supported. Replace it with a real adapter per OS; what each port needs
//! there is listed in docs/platforms.md.

use std::io;
use std::path::PathBuf;

use crate::browser::Family;

use super::{BrowserHosts, BrowserState, Desktop, Instance, InstanceGuard, Platform, Shell};

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "not supported on this system yet",
    ))
}

#[derive(Debug, Clone, Default)]
pub struct Unsupported;

impl Platform for Unsupported {
    fn from_process() -> Unsupported {
        Unsupported
    }
}

/// No single instance: every start runs on its own and nothing is handed over.
impl Instance for Unsupported {
    fn instance_name(&self) -> String {
        std::env::var("LIDER_SOCKET_NAME").unwrap_or_else(|_| "lider".into())
    }
    fn bind(
        &self,
        _name: &str,
        _on_line: Box<dyn Fn(String) + Send + Sync>,
    ) -> io::Result<InstanceGuard> {
        Ok(InstanceGuard::new(()))
    }
    fn send_lines(&self, _lines: &[String], _name: &str) -> bool {
        false
    }
    fn is_running(&self, _name: &str) -> bool {
        false
    }
}

impl Desktop for Unsupported {
    fn refresh(&self) -> Vec<&'static str> {
        vec![]
    }
    fn should_offer(&self, _menu_offered: bool) -> bool {
        false
    }
    fn system_installed(&self) -> bool {
        false
    }
    fn in_menu(&self) -> bool {
        false
    }
    fn add_to_menu(&self) -> io::Result<()> {
        unsupported()
    }
    fn remove_from_menu(&self) {}
    fn autostart_enabled(&self) -> bool {
        false
    }
    fn set_autostart(&self, _on: bool) -> io::Result<()> {
        unsupported()
    }
    fn self_contained_name(&self) -> Option<String> {
        None
    }
}

impl BrowserHosts for Unsupported {
    fn browsers(&self) -> Vec<BrowserState> {
        vec![]
    }
    fn install(&self, _names: &[&str]) -> io::Result<PathBuf> {
        unsupported()
    }
    fn uninstall(&self, _names: &[&str]) {}
    fn extension_folder(&self, _family: Family) -> io::Result<PathBuf> {
        unsupported()
    }
    fn exported_extension(&self, _family: Family) -> Option<PathBuf> {
        None
    }
}

impl Shell for Unsupported {
    fn open_path(&self, _path: &str) -> io::Result<()> {
        unsupported()
    }
    fn open_folder(&self, _path: &str) -> io::Result<()> {
        unsupported()
    }
    fn open_with(&self, _command: &str, _path: &str) -> io::Result<()> {
        unsupported()
    }
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()> {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
    }
    fn power_off(&self) -> io::Result<()> {
        unsupported()
    }
    fn prefers_dark(&self) -> Option<bool> {
        None
    }
    fn global_menu_available(&self) -> bool {
        false
    }
    fn font_families(&self) -> Vec<String> {
        vec![]
    }
    fn mono_family(&self) -> String {
        "monospace".into()
    }
}
