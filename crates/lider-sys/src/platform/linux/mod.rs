//! The Linux adapter: an abstract Unix socket for the single instance, XDG `.desktop` entries, browser
//! host manifests in each browser's folder, xdg-open / FileManager1 / the settings portal over D-Bus, and
//! fontconfig. This is the behaviour of 1.x, moved here unchanged.

pub mod browsers;
pub mod desktop;
pub mod env;
pub mod ipc;
pub mod shell;

use std::io;
use std::path::PathBuf;

use crate::browser::Family;

use super::{BrowserHosts, BrowserState, Desktop, Instance, InstanceGuard, Platform, Shell};
use env::{Launch, SysEnv};

/// Linux, for the user and folders in `env`.
#[derive(Debug, Clone)]
pub struct Linux {
    pub env: SysEnv,
}

impl Linux {
    pub fn new(env: SysEnv) -> Linux {
        Linux { env }
    }
}

impl Platform for Linux {
    fn from_process() -> Linux {
        Linux::new(SysEnv::from_process())
    }
}

impl Instance for Linux {
    fn instance_name(&self) -> String {
        ipc::socket_name()
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

impl Desktop for Linux {
    fn refresh(&self) -> Vec<&'static str> {
        desktop::refresh(&self.env)
    }
    fn should_offer(&self, menu_offered: bool) -> bool {
        desktop::should_offer(&self.env, menu_offered)
    }
    fn system_installed(&self) -> bool {
        desktop::system_installed(&self.env)
    }
    fn in_menu(&self) -> bool {
        desktop::in_menu(&self.env)
    }
    fn add_to_menu(&self) -> io::Result<()> {
        desktop::add_to_menu(&self.env)
    }
    fn remove_from_menu(&self) {
        desktop::remove_from_menu(&self.env)
    }
    fn autostart_enabled(&self) -> bool {
        desktop::autostart_enabled(&self.env)
    }
    fn set_autostart(&self, on: bool) -> io::Result<()> {
        desktop::set_autostart(&self.env, on)
    }
    fn self_contained_name(&self) -> Option<String> {
        match self.env.launch() {
            Launch::AppImage(p) => std::path::Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned()),
            _ => None,
        }
    }
}

impl BrowserHosts for Linux {
    fn browsers(&self) -> Vec<BrowserState> {
        let found = browsers::detected(&self.env);
        browsers::BROWSERS
            .iter()
            .map(|b| BrowserState {
                name: b.name,
                family: b.family,
                detected: found.contains(b),
                installed: browsers::is_installed(b, &self.env),
            })
            .collect()
    }
    fn install(&self, names: &[&str]) -> io::Result<PathBuf> {
        browsers::install(&browsers::by_name(names), &self.env)
    }
    fn uninstall(&self, names: &[&str]) {
        browsers::uninstall(&browsers::by_name(names), &self.env)
    }
    fn extension_folder(&self, family: Family) -> io::Result<PathBuf> {
        browsers::extension_folder(family, &self.env)
    }
    fn exported_extension(&self, family: Family) -> Option<PathBuf> {
        let p = self
            .env
            .dirs()
            .data
            .join("browser-extension")
            .join(family.as_str());
        p.join("manifest.json").is_file().then_some(p)
    }
}

impl Shell for Linux {
    fn open_path(&self, path: &str) -> io::Result<()> {
        shell::open_path(path)
    }
    fn open_folder(&self, path: &str) -> io::Result<()> {
        shell::open_folder(path)
    }
    fn open_with(&self, command: &str, path: &str) -> io::Result<()> {
        shell::open_with(command, path)
    }
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()> {
        shell::spawn_detached(program, args)
    }
    fn power_off(&self) -> io::Result<()> {
        shell::power_off()
    }
    fn prefers_dark(&self) -> Option<bool> {
        shell::prefers_dark()
    }
    fn global_menu_available(&self) -> bool {
        shell::global_menu_available()
    }
    fn font_families(&self) -> Vec<String> {
        shell::font_families()
    }
    fn mono_family(&self) -> String {
        shell::mono_family()
    }
}
