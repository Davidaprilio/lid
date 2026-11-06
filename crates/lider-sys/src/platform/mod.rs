//! The ports: everything LID asks of the operating system, as four interfaces. The rest of LID (the
//! window, the dialogs, the browser helper) only talks to these; each OS has one adapter that implements
//! all of them, picked when LID is compiled:
//!
//! | OS | adapter | state |
//! |---|---|---|
//! | Linux | [`linux::Linux`] | complete (what 1.x and 2.x do) |
//! | Windows | `windows::Windows` | named pipe, registry, Explorer (docs/platforms.md) |
//! | others (macOS) | `unsupported::Unsupported` | compiles; every service says "not supported yet" |
//!
//! A new OS (Windows, macOS) gets its own module next to `linux/` that implements [`Platform`], and one
//! line in [`Os`]. Nothing outside this module needs to change.
//!
//! What lives here, and what each adapter has to provide:
//! - [`Instance`]: one LID per user, and the line channel that hands it URLs and browser requests
//!   (Linux: an abstract Unix socket checked with `SO_PEERCRED`).
//! - [`Desktop`]: the application-menu entry and launching at login, for a copy no package installed
//!   (Linux: `.desktop` files that follow a moved AppImage).
//! - [`BrowserHosts`]: telling browsers where the native messaging host is (Linux: a manifest file in
//!   each browser's NativeMessagingHosts folder; Windows: registry keys).
//! - [`Shell`]: opening files and folders, "Open with…", powering off, starting a program detached,
//!   the system's light/dark choice, the global menu bar, the installed fonts.

use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::browser::Family;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(not(any(target_os = "linux", windows)))]
mod unsupported;
#[cfg(windows)]
pub mod windows;

/// The adapter of the OS LID is compiled for.
#[cfg(target_os = "linux")]
pub type Os = linux::Linux;
#[cfg(windows)]
pub type Os = windows::Windows;
#[cfg(not(any(target_os = "linux", windows)))]
pub type Os = unsupported::Unsupported;

/// `use lider_sys::platform::prelude::*` brings [`os`] and every port into scope.
pub mod prelude {
    pub use super::{BrowserHosts, Desktop, Instance, Platform, Shell, os};
}

/// The adapter for this process (read from the environment once).
pub fn os() -> &'static Os {
    static OS: OnceLock<Os> = OnceLock::new();
    OS.get_or_init(Os::from_process)
}

/// Every port at once: what an OS adapter implements.
pub trait Platform: Instance + Desktop + BrowserHosts + Shell + Send + Sync + 'static {
    /// The adapter for this process.
    fn from_process() -> Self
    where
        Self: Sized;
}

/// Keeps the instance name while it lives; dropping it frees the name for the next LID.
pub struct InstanceGuard(#[allow(dead_code)] Box<dyn Send + Sync>);

impl InstanceGuard {
    pub fn new(inner: impl Send + Sync + 'static) -> InstanceGuard {
        InstanceGuard(Box::new(inner))
    }
}

/// One LID per user. A second start hands its lines (URLs, `--show`, `{…}` browser requests) to the
/// running one, which passes each non-empty, trimmed line to `on_line`.
pub trait Instance {
    /// The name of this user's instance (`LIDER_SOCKET_NAME` overrides it).
    fn instance_name(&self) -> String;
    /// Take the name; fails when another LID holds it.
    fn bind(
        &self,
        name: &str,
        on_line: Box<dyn Fn(String) + Send + Sync>,
    ) -> io::Result<InstanceGuard>;
    /// Hand lines to the running LID (`--show` when there are none). False when none is running.
    fn send_lines(&self, lines: &[String], name: &str) -> bool;
    fn is_running(&self, name: &str) -> bool;
    /// One JSON request on one compact line.
    fn send_request(&self, request: &serde_json::Value, name: &str) -> bool {
        self.send_lines(&[request.to_string()], name)
    }
}

/// The application menu and launching at login, for a copy of LID that no package installed (an AppImage,
/// a portable copy). An installed package brings its own menu entry.
pub trait Desktop {
    /// Bring LID's own entries up to date with this copy (it may have moved); what changed, by name.
    fn refresh(&self) -> Vec<&'static str>;
    /// Ask once whether to add LID to the menu (first start of a copy that is not listed anywhere).
    fn should_offer(&self, menu_offered: bool) -> bool;
    /// A package put LID in the menu (the user's own entry is then not needed).
    fn system_installed(&self) -> bool;
    fn in_menu(&self) -> bool;
    fn add_to_menu(&self) -> io::Result<()>;
    fn remove_from_menu(&self);
    fn autostart_enabled(&self) -> bool;
    fn set_autostart(&self, on: bool) -> io::Result<()>;
    /// The file name of a self-contained copy (the AppImage) for "… runs on its own"; None otherwise.
    fn self_contained_name(&self) -> Option<String>;
}

/// A browser as the Browser integration window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserState {
    pub name: &'static str,
    pub family: Family,
    /// It has a profile on this computer.
    pub detected: bool,
    /// It knows where LID's native messaging host is.
    pub installed: bool,
}

/// Telling browsers where the native messaging host is, and exporting the extension for them.
pub trait BrowserHosts {
    /// Every browser this OS knows, in the order the window lists them.
    fn browsers(&self) -> Vec<BrowserState>;
    /// The browser names for "No supported browser profile found (…)".
    fn browser_list_text(&self) -> String {
        self.browsers()
            .iter()
            .map(|b| b.name.replace("Google ", "").replace("Microsoft ", ""))
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// Register the host for these browsers (by name); returns the program the browsers will run.
    fn install(&self, names: &[&str]) -> io::Result<PathBuf>;
    fn uninstall(&self, names: &[&str]);
    /// Export the extension for one family (for "Load unpacked"); returns its folder.
    fn extension_folder(&self, family: Family) -> io::Result<PathBuf>;
    /// The folder of an earlier export, if there is one (nothing is written).
    fn exported_extension(&self, family: Family) -> Option<PathBuf>;
}

/// Desktop services.
pub trait Shell {
    /// Open a file or folder with the default program.
    fn open_path(&self, path: &str) -> io::Result<()>;
    /// Show the file selected in the file manager (its folder when that is not possible).
    fn open_folder(&self, path: &str) -> io::Result<()>;
    /// "Open with…": the command as the user typed it, with the file appended.
    fn open_with(&self, command: &str, path: &str) -> io::Result<()>;
    /// Start a program that outlives LID and the browser (no shared console or session).
    fn spawn_detached(&self, program: &str, args: &[String]) -> io::Result<()>;
    fn power_off(&self) -> io::Result<()>;
    /// The system's colour choice: Some(true) dark, Some(false) light, None unknown.
    fn prefers_dark(&self) -> Option<bool>;
    /// A desktop-wide menu bar takes application menus (menu bar mode "Native").
    fn global_menu_available(&self) -> bool;
    /// Installed font families, sorted.
    fn font_families(&self) -> Vec<String>;
    /// The system's monospace family.
    fn mono_family(&self) -> String;
}
