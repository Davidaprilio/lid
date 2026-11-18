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
use lider_sys::platform::prelude::*;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::app::{self, AppState, UiRequest};

pub use lider_core::update::{Found, Prefs, RELEASES_API, is_dev_build, load, newest, save};

/// This copy can replace itself: an AppImage, or the Windows install.
pub fn self_updatable(state: &AppState) -> bool {
    cfg!(windows) || state.sys.self_contained_name().is_some()
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
