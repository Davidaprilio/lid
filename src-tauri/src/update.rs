//! Updates from LID's GitHub releases. At start (and with "Check now") LID reads the list of releases,
//! picks the newest one of the chosen channel (Stable, or Pre-release which adds the beta and alpha
//! versions) that is newer than this one, and offers it.
//!
//! - An AppImage and the Windows installer update themselves: `tauri-plugin-updater` downloads the file
//!   named in that release's `latest.json`, checks its minisign signature against the public key built
//!   into LID (`tauri.conf.json` → plugins.updater.pubkey), installs it and LID restarts. An update that is
//!   not signed with LID's key is refused.
//! - A deb or Arch install belongs to the package manager: LID only says that a new version is out and
//!   opens its release page.
//!
//! The choices live in `<config>/updates.json`, apart from 1.x's settings.json (1.x has no updates).

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use indexmap::IndexMap;
use lider_core::paths::Dirs;
use lider_sys::platform::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::app::{self, AppState, UiRequest};

pub const RELEASES_API: &str = "https://api.github.com/repos/Davidaprilio/lid/releases?per_page=30";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Look for a new version when LID starts.
    pub check_at_startup: bool,
    /// "stable", or "prerelease" (beta and alpha versions too).
    pub channel: String,
    /// "Skip this version": not offered again at start (Check now still shows it).
    pub skipped: String,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            check_at_startup: true,
            // Someone running a beta wants the next beta.
            channel: if lider_core::VERSION.contains('-') {
                "prerelease"
            } else {
                "stable"
            }
            .into(),
            skipped: String::new(),
        }
    }
}

fn prefs_file(dirs: &Dirs) -> std::path::PathBuf {
    dirs.config.join("updates.json")
}

pub fn load(dirs: &Dirs) -> Prefs {
    std::fs::read_to_string(prefs_file(dirs))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(dirs: &Dirs, p: &Prefs) -> std::io::Result<()> {
    std::fs::create_dir_all(&dirs.config)?;
    std::fs::write(
        prefs_file(dirs),
        serde_json::to_string_pretty(p).unwrap_or_default() + "\n",
    )
}

/// A newer release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Found {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// Its `latest.json` (what the updater reads); None when the release has none.
    pub latest_json: Option<String>,
    /// LID can install it itself (AppImage, Windows); otherwise the package manager does.
    pub can_install: bool,
}

/// The newest release of `channel` in a GitHub releases list that is newer than `current`.
pub fn newest(releases: &Value, channel: &str, current: &str) -> Option<Found> {
    let current = semver::Version::parse(current).ok()?;
    let mut best: Option<(semver::Version, &Value)> = None;
    for r in releases.as_array()? {
        if r["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        if r["prerelease"].as_bool().unwrap_or(false) && channel != "prerelease" {
            continue;
        }
        let Some(v) = r["tag_name"]
            .as_str()
            .and_then(|t| semver::Version::parse(t.trim_start_matches('v')).ok())
        else {
            continue;
        };
        // A stable channel ignores pre-release versions even if a release is not marked as one.
        if channel != "prerelease" && !v.pre.is_empty() {
            continue;
        }
        if v > current && best.as_ref().is_none_or(|(b, _)| v > *b) {
            best = Some((v, r));
        }
    }
    let (v, r) = best?;
    let latest_json = r["assets"].as_array().and_then(|a| {
        a.iter()
            .find(|x| x["name"] == "latest.json")
            .and_then(|x| x["browser_download_url"].as_str().map(String::from))
    });
    Some(Found {
        version: v.to_string(),
        notes: r["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(1500)
            .collect(),
        page: r["html_url"]
            .as_str()
            .unwrap_or(lider_sys::browser::RELEASES_URL)
            .into(),
        latest_json,
        can_install: false,
    })
}

/// This copy can replace itself: an AppImage, or the Windows install.
pub fn self_updatable(state: &AppState) -> bool {
    cfg!(windows) || state.sys.self_contained_name().is_some()
}

/// A build in a source checkout (target/…) never checks by itself.
fn is_dev_build() -> bool {
    std::env::current_exe().is_ok_and(|e| {
        e.ancestors()
            .skip(1)
            .any(|a| a.file_name().is_some_and(|n| n == "target"))
    })
}

pub async fn find(state: &AppState) -> Result<Option<Found>, String> {
    let client = state.manager().client().map_err(|e| e.message())?;
    let channel = load(&state.dirs).channel;
    let mut headers = IndexMap::new();
    headers.insert(
        "Accept".to_string(),
        "application/vnd.github+json".to_string(),
    );
    let fut = async move {
        let mut resp = client
            .open(RELEASES_API, 0, false, &headers)
            .await
            .map_err(|e| e.message())?;
        if resp.status != 200 {
            return Err(format!("GitHub answered HTTP {}", resp.status));
        }
        let mut data = Vec::new();
        while let Some(c) = resp
            .body
            .chunk(Duration::from_secs(20))
            .await
            .map_err(|e| e.message())?
        {
            data.extend_from_slice(&c);
        }
        serde_json::from_slice::<Value>(&data).map_err(|e| e.to_string())
    };
    let releases = state
        ._engine
        .handle()
        .spawn(fut)
        .await
        .map_err(|e| e.to_string())??;
    Ok(
        newest(&releases, &channel, lider_core::VERSION).map(|mut f| {
            f.can_install = self_updatable(state) && f.latest_json.is_some();
            f
        }),
    )
}

/// At start: a quiet check after a few seconds; only a newer, not skipped version shows up.
pub fn check_at_startup(app: &AppHandle) {
    let state = app.state::<AppState>();
    let prefs = load(&state.dirs);
    let forced = std::env::var("LIDER_UPDATE_CHECK").is_ok_and(|v| v == "1");
    if !prefs.check_at_startup
        || (!forced && (is_dev_build() || std::env::var("LIDER_E2E").is_ok()))
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let state = app.state::<AppState>();
        if let Ok(Some(f)) = find(state.inner()).await
            && f.version != load(&state.dirs).skipped
        {
            app::request(&app, UiRequest::UpdateAvailable(f));
        }
    });
}

static PROGRESS: (AtomicI64, AtomicI64) = (AtomicI64::new(0), AtomicI64::new(-1));

#[tauri::command]
pub fn update_prefs(state: tauri::State<'_, AppState>) -> Prefs {
    load(&state.dirs)
}

#[tauri::command]
pub fn update_set_prefs(state: tauri::State<'_, AppState>, prefs: Prefs) -> Result<(), String> {
    save(&state.dirs, &prefs).map_err(|e| e.to_string())
}

/// "Check now": the newer version, or None when this one is the newest.
#[tauri::command]
pub async fn update_check(state: tauri::State<'_, AppState>) -> Result<Option<Found>, String> {
    find(state.inner()).await
}

/// Show a found version in the main window (from Options → Check now).
#[tauri::command]
pub fn update_offer(app: AppHandle, found: Found) {
    app::show_normal(&app);
    app::request(&app, UiRequest::UpdateAvailable(found));
}

#[tauri::command]
pub fn update_skip(state: tauri::State<'_, AppState>, version: String) -> Result<(), String> {
    let mut p = load(&state.dirs);
    p.skipped = version;
    save(&state.dirs, &p).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_progress() -> (i64, i64) {
    (
        PROGRESS.0.load(Ordering::Relaxed),
        PROGRESS.1.load(Ordering::Relaxed),
    )
}

/// Download, check the signature, install, restart. Errors come back as text (nothing is installed).
#[tauri::command]
pub async fn update_install(app: AppHandle, latest_json: String) -> Result<(), String> {
    let url = latest_json
        .parse()
        .map_err(|e: url::ParseError| e.to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("LID is up to date")?;
    PROGRESS.0.store(0, Ordering::Relaxed);
    PROGRESS.1.store(-1, Ordering::Relaxed);
    update
        .download_and_install(
            |chunk, total| {
                PROGRESS.0.fetch_add(chunk as i64, Ordering::Relaxed);
                PROGRESS
                    .1
                    .store(total.map_or(-1, |t| t as i64), Ordering::Relaxed);
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    app.state::<AppState>().manager().shutdown();
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rel(tag: &str, pre: bool) -> Value {
        json!({"tag_name": tag, "prerelease": pre, "draft": false, "html_url": format!("https://x/{tag}"), "body": "notes",
               "assets": [{"name": "latest.json", "browser_download_url": format!("https://x/{tag}/latest.json")}]})
    }

    #[test]
    fn picks_the_newest_of_the_channel() {
        let list = json!([
            rel("v2.0.0-beta.2", true),
            rel("v1.2.0", false),
            rel("v2.0.0-beta.1", true),
            rel("nightly", true)
        ]);
        assert!(
            newest(&list, "stable", "1.2.0").is_none(),
            "no stable release is newer"
        );
        let f = newest(&list, "prerelease", "1.2.0").unwrap();
        assert_eq!(f.version, "2.0.0-beta.2");
        assert_eq!(
            f.latest_json.as_deref(),
            Some("https://x/v2.0.0-beta.2/latest.json")
        );
        assert!(
            newest(&list, "prerelease", "2.0.0-beta.2").is_none(),
            "this one is the newest"
        );
        let list = json!([rel("v2.0.0", false), rel("v2.0.1-rc.1", true)]);
        assert_eq!(
            newest(&list, "stable", "2.0.0-beta.2").unwrap().version,
            "2.0.0"
        );
        assert_eq!(
            newest(&list, "prerelease", "2.0.0").unwrap().version,
            "2.0.1-rc.1"
        );
    }

    #[test]
    fn drafts_and_unmarked_pre_versions_are_not_stable() {
        let mut draft = rel("v3.0.0", false);
        draft["draft"] = json!(true);
        let list = json!([draft, rel("v2.1.0-alpha.1", false)]);
        assert!(newest(&list, "stable", "2.0.0").is_none());
    }
}
