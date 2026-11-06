//! Where LID keeps its files, and the move from the folder names of earlier builds
//! (`settings.py` `config_dir`/`data_dir`/`cache_dir`/`migrate_legacy_dirs` in 1.x).
//!
//! The folders are passed around as a [`Dirs`] value instead of being read from the environment at
//! every use, so tests can point them anywhere without touching process-wide state.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::PACKAGE_NAME;

/// Folder names used by earlier builds, newest first.
pub const LEGACY_NAMES: [&str; 2] = ["lidman", "lindm"];

/// LID's folders: `<XDG dir>/lider` for config, data and cache, plus the home folder. The XDG variables
/// win on every OS; without them each OS has its own place (see [`default_homes`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME/lider`, holds `settings.json`.
    pub config: PathBuf,
    /// `$XDG_DATA_HOME/lider`, holds `downloads.db`.
    pub data: PathBuf,
    /// `$XDG_CACHE_HOME/lider`, holds `parts/`.
    pub cache: PathBuf,
}

impl Dirs {
    /// The folders from the environment, the way 1.x reads them. An empty variable counts as unset.
    pub fn from_env() -> Dirs {
        let home = home_dir();
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        Dirs::from_parts(
            home,
            var("XDG_CONFIG_HOME"),
            var("XDG_DATA_HOME"),
            var("XDG_CACHE_HOME"),
        )
    }

    /// The folders for a given home and XDG variables (`None` = unset).
    pub fn from_parts(
        home: PathBuf,
        config_home: Option<OsString>,
        data_home: Option<OsString>,
        cache_home: Option<OsString>,
    ) -> Dirs {
        let pick = |v: Option<OsString>, default: PathBuf| {
            v.map(PathBuf::from).unwrap_or(default).join(PACKAGE_NAME)
        };
        let [config, data, cache] = default_homes(&home);
        Dirs {
            config: pick(config_home, config),
            data: pick(data_home, data),
            cache: pick(cache_home, cache),
            home,
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }

    pub fn database_file(&self) -> PathBuf {
        self.data.join("downloads.db")
    }

    /// The default folder for part files (`temp_dir`).
    pub fn parts(&self) -> PathBuf {
        self.cache.join("parts")
    }

    /// `~/Downloads`: the home folder's `Downloads`, not the XDG user dir, as in 1.x.
    pub fn downloads_root(&self) -> PathBuf {
        self.home.join("Downloads")
    }
}

/// Where config, data and cache go without the XDG variables (each gets a `lider` folder):
/// Linux and other Unix `~/.config`, `~/.local/share`, `~/.cache` (1.x); macOS `~/Library/Application
/// Support` (config and data) and `~/Library/Caches`; Windows `AppData\Roaming` (config) and
/// `AppData\Local` (data and cache) in the home folder.
pub fn default_homes(home: &Path) -> [PathBuf; 3] {
    if cfg!(windows) {
        let local = home.join("AppData").join("Local");
        [home.join("AppData").join("Roaming"), local.clone(), local]
    } else if cfg!(target_os = "macos") {
        let support = home.join("Library").join("Application Support");
        [
            support.clone(),
            support,
            home.join("Library").join("Caches"),
        ]
    } else {
        [
            home.join(".config"),
            home.join(".local").join("share"),
            home.join(".cache"),
        ]
    }
}

/// `$HOME`, like Python's `Path.home()`; the password database only when it is unset.
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(std::env::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Move settings, the download list and part files out of an earlier build's folders.
///
/// For each of config, data and cache: a legacy sibling folder is renamed to the new name, or merged
/// into it when the new one already exists (items already present stay; the old folder is removed
/// only when it ends up empty). Afterwards the old cache paths in `settings.json` are rewritten, which
/// fixes `temp_dir`. Errors are ignored the way 1.x lets them pass: a failed move leaves files in place.
pub fn migrate_legacy_dirs(dirs: &Dirs) {
    for new in [&dirs.config, &dirs.data, &dirs.cache] {
        for name in LEGACY_NAMES {
            let old = new.with_file_name(name);
            if !old.is_dir() {
                continue;
            }
            if !new.exists() {
                let _ = fs::rename(&old, new);
                continue;
            }
            if let Ok(items) = fs::read_dir(&old) {
                for item in items.flatten() {
                    let target = new.join(item.file_name());
                    if !target.exists() {
                        let _ = fs::rename(item.path(), target);
                    }
                }
            }
            let _ = fs::remove_dir(&old); // something with the same name already existed: leave it
        }
    }
    rewrite_legacy_cache_paths(dirs);
}

fn rewrite_legacy_cache_paths(dirs: &Dirs) {
    let cfg = dirs.settings_file();
    let Ok(text) = fs::read_to_string(&cfg) else {
        return;
    };
    let new = dirs.cache.to_string_lossy().into_owned();
    let mut changed = text.clone();
    for name in LEGACY_NAMES {
        let old = dirs.cache.with_file_name(name);
        changed = changed.replace(&*old.to_string_lossy(), &new);
    }
    if changed != text {
        let _ = fs::write(&cfg, changed);
    }
}

/// `os.path.join(a, b)` for display paths kept as strings in the saved data.
pub fn join(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs_in(root: &Path) -> Dirs {
        Dirs::from_parts(
            root.join("home"),
            Some(root.join("c").into()),
            Some(root.join("d").into()),
            Some(root.join("k").into()),
        )
    }

    #[cfg(windows)]
    #[test]
    fn windows_folders_without_xdg() {
        let d = Dirs::from_parts(r"C:\Users\u".into(), None, None, None);
        assert_eq!(d.config, PathBuf::from(r"C:\Users\u\AppData\Roaming\lider"));
        assert_eq!(d.data, PathBuf::from(r"C:\Users\u\AppData\Local\lider"));
        assert_eq!(
            d.parts(),
            PathBuf::from(r"C:\Users\u\AppData\Local\lider\parts")
        );
    }

    // The layout of 1.x on Linux (Unix paths and folders).
    #[cfg(unix)]
    #[test]
    fn xdg_defaults_and_overrides() {
        let d = Dirs::from_parts(PathBuf::from("/h"), None, None, None);
        assert_eq!(d.config, PathBuf::from("/h/.config/lider"));
        assert_eq!(d.data, PathBuf::from("/h/.local/share/lider"));
        assert_eq!(d.cache, PathBuf::from("/h/.cache/lider"));
        assert_eq!(d.parts(), PathBuf::from("/h/.cache/lider/parts"));
        assert_eq!(
            d.settings_file(),
            PathBuf::from("/h/.config/lider/settings.json")
        );
        assert_eq!(
            d.database_file(),
            PathBuf::from("/h/.local/share/lider/downloads.db")
        );
        assert_eq!(d.downloads_root(), PathBuf::from("/h/Downloads"));
        let d = Dirs::from_parts(PathBuf::from("/h"), Some("/x".into()), None, None);
        assert_eq!(d.config, PathBuf::from("/x/lider"));
    }

    /// Port of test_manager.py `test_legacy_lindm_folders_are_migrated`.
    // The layout of 1.x on Linux (Unix paths and folders).
    #[cfg(unix)]
    #[test]
    fn legacy_lindm_folders_are_migrated() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        for base in ["c", "d", "k"] {
            fs::create_dir_all(t.join(base).join("lindm")).unwrap();
        }
        fs::write(
            t.join("c/lindm/settings.json"),
            format!(
                r#"{{"temp_dir": "{}/k/lindm/parts", "max_connections": 16}}"#,
                t.display()
            ),
        )
        .unwrap();
        fs::write(t.join("d/lindm/downloads.db"), "").unwrap();
        fs::create_dir_all(t.join("d/lider/native-host")).unwrap(); // set up before the first start: must merge
        let dirs = dirs_in(t);
        migrate_legacy_dirs(&dirs);
        assert!(!t.join("c/lindm").exists());
        assert!(t.join("d/lider/downloads.db").exists());
        assert!(!t.join("d/lindm").exists());
        let s = crate::settings::load(&dirs);
        assert_eq!(s.max_connections, 16);
        assert_eq!(s.temp_dir, format!("{}/k/lider/parts", t.display()));
    }

    #[test]
    fn merge_keeps_existing_items_and_the_old_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        fs::create_dir_all(t.join("d/lidman")).unwrap();
        fs::create_dir_all(t.join("d/lider")).unwrap();
        fs::write(t.join("d/lidman/downloads.db"), "old").unwrap();
        fs::write(t.join("d/lidman/extra"), "x").unwrap();
        fs::write(t.join("d/lider/downloads.db"), "new").unwrap();
        migrate_legacy_dirs(&dirs_in(t));
        assert_eq!(
            fs::read_to_string(t.join("d/lider/downloads.db")).unwrap(),
            "new"
        );
        assert!(t.join("d/lider/extra").exists());
        assert!(t.join("d/lidman/downloads.db").exists()); // not empty, so it stays
    }
}
