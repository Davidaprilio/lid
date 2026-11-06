//! The parts of the environment that decide how this copy of LID is started and where its files go.
//! Read once from the process ([`SysEnv::from_process`]); tests build their own.

use std::path::{Path, PathBuf};

use lider_core::paths::{Dirs, home_dir};

use crate::sh;

/// How this copy of LID runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// An AppImage (`$APPIMAGE`).
    AppImage(String),
    /// A `lider` found on PATH (a package or `cargo install`).
    Installed(PathBuf),
    /// A build in a source checkout (`…/target/debug/lider`): it must not take over the user's entries.
    Checkout(PathBuf),
    /// Any other copy of the binary.
    Binary(PathBuf),
}

#[derive(Debug, Clone)]
pub struct SysEnv {
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME` or `~/.config`.
    pub config_home: PathBuf,
    /// `$XDG_DATA_HOME` or `~/.local/share`.
    pub data_home: PathBuf,
    pub appimage: Option<String>,
    /// `lider` on PATH.
    pub installed: Option<PathBuf>,
    pub current_exe: PathBuf,
    /// The packages' menu entry.
    pub system_entry: PathBuf,
    /// `MOZ_LEGACY_HOME` is set.
    pub moz_legacy_home: bool,
    /// Run `update-desktop-database` after menu changes.
    pub update_menus: bool,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

impl SysEnv {
    pub fn from_process() -> SysEnv {
        let home = home_dir();
        SysEnv {
            config_home: var("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from),
            data_home: var("XDG_DATA_HOME")
                .map_or_else(|| home.join(".local/share"), PathBuf::from),
            appimage: var("APPIMAGE"),
            installed: which("lider"),
            current_exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("lider")),
            system_entry: PathBuf::from("/usr/share/applications/lider.desktop"),
            moz_legacy_home: var("MOZ_LEGACY_HOME").is_some(),
            update_menus: true,
            home,
        }
    }

    /// A test environment: everything below `root`, no PATH lookups, no menu cache updates.
    pub fn for_test(root: &Path, appimage: Option<&str>) -> SysEnv {
        SysEnv {
            home: root.to_path_buf(),
            config_home: root.join("config"),
            data_home: root.join("data"),
            appimage: appimage.map(String::from),
            installed: None,
            current_exe: PathBuf::from("/usr/local/bin/lider"),
            system_entry: root.join("usr/lider.desktop"),
            moz_legacy_home: false,
            update_menus: false,
        }
    }

    /// LID's own folders (settings, data, cache) for this environment.
    pub fn dirs(&self) -> Dirs {
        Dirs::from_parts(
            self.home.clone(),
            Some(self.config_home.clone().into_os_string()),
            Some(self.data_home.clone().into_os_string()),
            var("XDG_CACHE_HOME").map(Into::into),
        )
    }

    pub fn launch(&self) -> Launch {
        if let Some(a) = &self.appimage {
            return Launch::AppImage(a.clone());
        }
        if let Some(p) = &self.installed {
            return Launch::Installed(p.clone());
        }
        let exe = self.current_exe.clone();
        let in_target = exe
            .ancestors()
            .skip(1)
            .any(|a| a.file_name().is_some_and(|n| n == "target"));
        if in_target {
            Launch::Checkout(exe)
        } else {
            Launch::Binary(exe)
        }
    }

    /// The program that starts this copy of LID.
    pub fn launch_program(&self) -> String {
        match self.launch() {
            Launch::AppImage(a) => a,
            Launch::Installed(p) | Launch::Checkout(p) | Launch::Binary(p) => {
                p.to_string_lossy().into_owned()
            }
        }
    }

    /// `launch_program` plus `extra`, as a shell command line.
    pub fn launch_command(&self, extra: &[&str]) -> String {
        let mut args = vec![self.launch_program()];
        args.extend(extra.iter().map(|s| s.to_string()));
        sh::join(&args)
    }
}
