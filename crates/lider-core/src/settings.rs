//! Application options and their defaults (`settings.py` in 1.x), saved as
//! `~/.config/lider/settings.json` with the same keys.
//!
//! Loading follows 1.x: unknown keys are ignored, missing keys take their default, the migration shims
//! for older files run first, and a file that cannot be read gives the full defaults. One difference:
//! 1.x stored a value of the wrong JSON type as it was (and could fail later); here that one key falls
//! back to its default (decision Q14 in docs/tauri/PLAN.md).

use std::fs;
use std::io;
use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::paths::Dirs;
use crate::pyutil::{fnmatch, truthy};

/// The connection counts offered in Options -> Connection.
pub const CONNECTION_CHOICES: [i64; 7] = [1, 2, 4, 8, 16, 24, 32];

pub const DEFAULT_FILE_TYPES: &str = "3GP 7Z AAC ACE AIF APK ARJ ASF AVI BIN BZ2 EXE GZ GZIP IMG ISO LZH M4A M4V MKV MOV MP3 MP4 \
MPA MPE MPEG MPG MSI MSU OGG OGV PDF PLJ PPS PPT QT R0* R1* RA RAR RM RMVB SEA SIT SITX TAR \
TIF TIFF WAV WMA WMV Z ZIP";

pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

const DEFAULT_CATEGORIES: [(&str, &str); 5] = [
    (
        "Compressed",
        "zip rar r0* r1* arj gz sit sitx sea ace bz2 7z",
    ),
    ("Documents", "doc pdf ppt pps docx pptx"),
    ("Music", "mp3 wav wma mpa ram ra aac aif m4a tsa"),
    ("Programs", "exe msi"),
    (
        "Video",
        "avi mpg mpe mpeg asf wmv mov qt rm mp4 flv m4v webm ogv ogg mkv ts tsv",
    ),
];

/// A column of the download list: key, title, visible by default, width; in the default order.
pub struct ColumnDef {
    pub key: &'static str,
    pub title: &'static str,
    pub visible: bool,
    pub width: i64,
}

pub const DEFAULT_COLUMNS: [ColumnDef; 12] = [
    col("queue", "Q", true, 34),
    col("filename", "File Name", true, 280),
    col("size", "Size", true, 92),
    col("status", "Status", true, 180),
    col("time_left", "Time left", true, 100),
    col("rate", "Transfer rate", true, 120),
    col("last_try", "Last Try Date", true, 130),
    col("description", "Description", true, 200),
    col("date_added", "Date Added", false, 130),
    col("save_to", "Save To", false, 200),
    col("referer", "Referer", false, 200),
    col("parent_page", "Parent Web Page", false, 200),
];

const fn col(key: &'static str, title: &'static str, visible: bool, width: i64) -> ColumnDef {
    ColumnDef {
        key,
        title,
        visible,
        width,
    }
}

/// A saved column: `[key, visible, width]`, as 1.x writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnState(pub String, pub bool, pub i64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub save_dir: String,
    #[serde(default)]
    pub sites: Vec<String>,
    #[serde(default)]
    pub remember_last: bool,
}

pub fn default_categories(dirs: &Dirs) -> Vec<Category> {
    let root = dirs.downloads_root();
    let mut cats = vec![Category {
        name: "General".into(),
        extensions: vec![],
        save_dir: root.to_string_lossy().into_owned(),
        sites: vec![],
        remember_last: false,
    }];
    cats.extend(DEFAULT_CATEGORIES.iter().map(|(name, exts)| Category {
        name: (*name).into(),
        extensions: exts.split_whitespace().map(String::from).collect(),
        save_dir: root.join(name).to_string_lossy().into_owned(),
        sites: vec![],
        remember_last: false,
    }));
    cats
}

fn default_columns() -> Vec<ColumnState> {
    DEFAULT_COLUMNS
        .iter()
        .map(|c| ColumnState(c.key.into(), c.visible, c.width))
        .collect()
}

/// Declares `Settings` once: the struct in 1.x field order (which is also the order in the saved
/// file), its defaults, and the per-key loader.
macro_rules! settings {
    ($dirs:ident; $( $(#[$doc:meta])* $name:ident : $ty:ty = $default:expr ),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Serialize)]
        pub struct Settings {
            $( $(#[$doc])* pub $name: $ty, )*
        }

        impl Settings {
            /// The defaults; folders come from `dirs`.
            pub fn defaults($dirs: &Dirs) -> Settings {
                Settings { $( $name: $default, )* }
            }

            /// Set every known key of `d` whose value has the right type; others keep their value.
            fn apply(&mut self, d: &Map<String, Value>) -> Result<(), LoadError> {
                $(
                    if let Some(v) = d.get(stringify!($name)) {
                        if stringify!($name) == "categories" {
                            // 1.x builds Category(**c): an unknown or missing key fails the whole file.
                            self.categories = serde_json::from_value(v.clone())
                                .map_err(|_| LoadError::Categories)?;
                        } else if let Ok(x) = serde_json::from_value::<$ty>(v.clone()) {
                            self.$name = x;
                        }
                    }
                )*
                Ok(())
            }

            /// Every key of the saved file, in order.
            pub const KEYS: &'static [&'static str] = &[ $( stringify!($name), )* ];
        }
    };
}

settings! {
    dirs;
    // Options -> General
    launch_on_startup: bool = false,
    /// An AppImage offered once to add itself to the application menu.
    menu_offered: bool = false,
    monitor_clipboard: bool = true,
    /// Take over browser downloads through the LID extension.
    browser_capture: bool = true,
    // Options -> File types
    auto_capture_types: String = DEFAULT_FILE_TYPES.into(),
    no_auto_sites: String = String::new(),
    // Options -> Downloads
    show_start_dialog: bool = true,
    only_add_to_queue: bool = false,
    show_complete_dialog: bool = true,
    /// ask | number | overwrite | complete
    duplicate_action: String = "ask".into(),
    /// When a download has a checksum, check it once the file is complete.
    verify_checksum: bool = true,
    /// Fill the File Info checksum field from a hash on the clipboard.
    checksum_from_clipboard: bool = false,
    /// ask | keep | redownload
    checksum_mismatch: String = "ask".into(),
    user_agent: String = DEFAULT_USER_AGENT.into(),
    // Options -> Connection
    max_connections: i64 = 8,
    /// Host pattern -> connections; the first matching pattern in saved order wins.
    server_limits: IndexMap<String, i64> = IndexMap::new(),
    retries: i64 = 10,
    retry_delay: f64 = 3.0,
    timeout: f64 = 30.0,
    // Options -> Save to
    categories: Vec<Category> = default_categories(dirs),
    temp_dir: String = dirs.parts().to_string_lossy().into_owned(),
    // Options -> Proxy
    proxy: String = String::new(),
    // Speed limiter (Downloads menu)
    speed_limiter_on: bool = false,
    speed_limit_kbps: i64 = 200,
    speed_limiter_on_startup: bool = false,
    // View / Options -> Appearance
    columns: Vec<ColumnState> = default_columns(),
    /// ember | ember-dark | dark, used when not following the system.
    theme: String = "ember".into(),
    follow_system: bool = true,
    /// The theme used when following a dark system.
    dark_theme: String = "ember-dark".into(),
    /// comfortable | compact
    density: String = "comfortable".into(),
    toolbar_labels: bool = true,
    /// native | custom | both
    menubar: String = "native".into(),
    show_categories: bool = true,
    show_details: bool = true,
    show_statusbar: bool = true,
    tray_icon: bool = true,
    close_to_tray: bool = true,
    start_minimized: bool = false,
    /// A Qt font string from 1.x ("Family,size,…"); see `list_font_family_and_size`.
    list_font: String = String::new(),
    sort_key: String = "date_added".into(),
    sort_desc: bool = false,
    /// Qt's saved geometry from 1.x; kept so 1.x can still use it, ignored by 2.x (see `window_state`).
    window_geometry: String = String::new(),
    sidebar_width: i64 = 236,
    details_height: i64 = 210,
    // Search (last query and options)
    search_flags: i64 = 0x04,
    search_string: String = String::new(),
    // Progress dialog
    show_progress_dialog: bool = true,
    // YouTube: the variant picked last time
    media_kind: String = "video".into(),
    media_quality: String = String::new(),
    // New in 2.x. 1.x ignores unknown keys, so these are safe to add.
    /// The main window's size, position and maximised state, written by 2.x.
    window_state: Option<WindowState> = None,
}

/// The main window's geometry as 2.x saves it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read the settings file: {0}")]
    Io(#[from] io::Error),
    #[error("the settings file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the settings file does not hold a JSON object")]
    NotAnObject,
    #[error("a category in the settings file is not valid")]
    Categories,
}

impl Default for Settings {
    /// The defaults with the folders from the environment.
    fn default() -> Self {
        Settings::defaults(&Dirs::from_env())
    }
}

impl Settings {
    /// The category called `name`, or the first one (General).
    pub fn category(&self, name: &str) -> &Category {
        self.categories
            .iter()
            .find(|c| c.name == name)
            .unwrap_or(&self.categories[0])
    }

    pub fn category_mut(&mut self, name: &str) -> &mut Category {
        let i = self
            .categories
            .iter()
            .position(|c| c.name == name)
            .unwrap_or(0);
        &mut self.categories[i]
    }

    /// Connections for a host: the first `server_limits` pattern that matches, else `max_connections`.
    pub fn connections_for(&self, host: &str) -> i64 {
        let host = host.to_lowercase();
        self.server_limits
            .iter()
            .find(|(pattern, _)| fnmatch(&host, &pattern.to_lowercase()))
            .map_or(self.max_connections, |(_, n)| *n)
    }

    /// Family and point size from the 1.x Qt font string `"Family,size,…"`; `None` for the default font.
    pub fn list_font_family_and_size(&self) -> Option<(String, f64)> {
        let mut parts = self.list_font.split(',');
        let family = parts.next()?.trim();
        if family.is_empty() {
            return None;
        }
        let size = parts
            .next()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|s| *s > 0.0)?;
        Some((family.to_string(), size))
    }

    /// Settings from the parsed file, with the 1.x migration shims.
    pub fn from_json(d: &Value, dirs: &Dirs) -> Result<Settings, LoadError> {
        let mut d = d.as_object().ok_or(LoadError::NotAnObject)?.clone();
        // Settings written before the themes existed.
        if !d.contains_key("theme") && d.get("dark_mode").is_some_and(truthy) {
            d.insert("theme".into(), "ember-dark".into());
            d.insert("follow_system".into(), false.into());
        }
        if !d.contains_key("toolbar_labels")
            && let Some(large) = d.get("toolbar_large")
        {
            let labels = truthy(large);
            d.insert("toolbar_labels".into(), labels.into());
        }
        let mut s = Settings::defaults(dirs);
        s.apply(&d)?;
        // Columns added in a newer version must still appear.
        for c in &DEFAULT_COLUMNS {
            if !s.columns.iter().any(|have| have.0 == c.key) {
                s.columns
                    .push(ColumnState(c.key.into(), c.visible, c.width));
            }
        }
        Ok(s)
    }

    pub fn to_json(&self) -> Value {
        let mut v = serde_json::to_value(self).expect("Settings always serialise");
        if self.window_state.is_none()
            && let Some(map) = v.as_object_mut()
        {
            map.remove("window_state");
        }
        v
    }
}

/// Read a settings file; an error for anything that 1.x would replace with the defaults.
pub fn try_load_from(path: &Path, dirs: &Dirs) -> Result<Settings, LoadError> {
    let text = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    Settings::from_json(&value, dirs)
}

/// `settings.json` from the config folder, or the defaults when it is missing or unreadable.
pub fn load(dirs: &Dirs) -> Settings {
    load_from(&dirs.settings_file(), dirs)
}

pub fn load_from(path: &Path, dirs: &Dirs) -> Settings {
    try_load_from(path, dirs).unwrap_or_else(|_| Settings::defaults(dirs))
}

/// Save to `settings.json` in the config folder.
pub fn save(settings: &Settings, dirs: &Dirs) -> io::Result<()> {
    save_to(settings, &dirs.settings_file())
}

/// Write `settings.tmp` next to `path` (indented JSON), then replace `path` with it.
pub fn save_to(settings: &Settings, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(&settings.to_json()).map_err(io::Error::other)?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dirs() -> Dirs {
        Dirs::from_parts("/home/u".into(), None, None, None)
    }

    // The layout of 1.x on Linux (Unix paths and folders).
    #[cfg(unix)]
    #[test]
    fn defaults_match_1x() {
        let s = Settings::defaults(&dirs());
        assert_eq!(s.temp_dir, "/home/u/.cache/lider/parts");
        let names: Vec<_> = s.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "General",
                "Compressed",
                "Documents",
                "Music",
                "Programs",
                "Video"
            ]
        );
        assert_eq!(s.categories[0].save_dir, "/home/u/Downloads");
        assert_eq!(s.categories[5].save_dir, "/home/u/Downloads/Video");
        assert_eq!(s.columns.len(), 12);
        assert_eq!((s.max_connections, s.retries, s.search_flags), (8, 10, 4));
        // 49 keys from 1.x plus window_state.
        assert_eq!(Settings::KEYS.len(), 50);
        assert!(s.to_json().get("window_state").is_none());
    }

    /// Port of test_manager.py `test_old_dark_mode_and_toolbar_size_map_to_themes`.
    #[test]
    fn old_dark_mode_and_toolbar_size_map_to_themes() {
        let s = Settings::from_json(&json!({"dark_mode": true, "toolbar_large": false}), &dirs())
            .unwrap();
        assert_eq!(
            (s.theme.as_str(), s.follow_system, s.toolbar_labels),
            ("ember-dark", false, false)
        );
        let s = Settings::from_json(&json!({}), &dirs()).unwrap();
        assert_eq!(
            (s.theme.as_str(), s.follow_system, s.dark_theme.as_str()),
            ("ember", true, "ember-dark")
        );
    }

    /// Port of test_manager.py `test_settings_round_trip`.
    #[test]
    fn settings_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("s.json");
        let mut s = Settings::defaults(&dirs());
        s.max_connections = 16;
        s.category_mut("Video").extensions.push("vob".into());
        save_to(&s, &path).unwrap();
        assert!(!tmp.path().join("s.tmp").exists());
        let back = load_from(&path, &dirs());
        assert_eq!(back.max_connections, 16);
        assert!(
            back.category("Video")
                .extensions
                .contains(&"vob".to_string())
        );
        assert_eq!(back, s);
    }

    #[test]
    fn bad_files_give_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("s.json");
        assert_eq!(load_from(&path, &dirs()), Settings::defaults(&dirs()));
        fs::write(&path, "{not json").unwrap();
        assert_eq!(load_from(&path, &dirs()), Settings::defaults(&dirs()));
        fs::write(&path, "[1, 2]").unwrap();
        assert_eq!(load_from(&path, &dirs()), Settings::defaults(&dirs()));
        // An unknown key in a category fails the whole file, as Category(**c) does in 1.x.
        fs::write(
            &path,
            r#"{"max_connections": 2, "categories": [{"name": "X", "colour": 1}]}"#,
        )
        .unwrap();
        assert_eq!(load_from(&path, &dirs()), Settings::defaults(&dirs()));
    }

    #[test]
    fn wrong_type_keeps_that_default_only() {
        let s = Settings::from_json(
            &json!({"max_connections": "many", "retries": 3, "unknown": 1}),
            &dirs(),
        )
        .unwrap();
        assert_eq!((s.max_connections, s.retries), (8, 3));
    }

    #[test]
    fn missing_columns_are_appended() {
        let s = Settings::from_json(
            &json!({"columns": [["status", true, 99], ["filename", false, 10]]}),
            &dirs(),
        )
        .unwrap();
        assert_eq!(s.columns[0], ColumnState("status".into(), true, 99));
        assert_eq!(s.columns[1], ColumnState("filename".into(), false, 10));
        assert_eq!(s.columns.len(), 12);
        assert_eq!(s.columns[2], ColumnState("queue".into(), true, 34));
    }

    #[test]
    fn server_limits_first_match_in_saved_order() {
        let s = Settings::from_json(
            &json!({"server_limits": {"*.Example.com": 4, "*": 1}, "max_connections": 8}),
            &dirs(),
        )
        .unwrap();
        assert_eq!(s.connections_for("CDN.example.com"), 4);
        assert_eq!(s.connections_for("other.org"), 1);
        assert_eq!(Settings::defaults(&dirs()).connections_for("x"), 8);
    }

    #[test]
    fn category_lookup_falls_back_to_general() {
        let s = Settings::defaults(&dirs());
        assert_eq!(s.category("Music").name, "Music");
        assert_eq!(s.category("Nope").name, "General");
    }

    #[test]
    fn list_font_from_qt_string() {
        let mut s = Settings::defaults(&dirs());
        assert_eq!(s.list_font_family_and_size(), None);
        s.list_font = "Inter,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1".into();
        assert_eq!(s.list_font_family_and_size(), Some(("Inter".into(), 10.0)));
        s.list_font = "Noto Sans,10.5".into();
        assert_eq!(
            s.list_font_family_and_size(),
            Some(("Noto Sans".into(), 10.5))
        );
    }
}
