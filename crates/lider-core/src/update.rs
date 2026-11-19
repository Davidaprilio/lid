//! Updates from LID's GitHub releases. At start (and with "Check now") LID reads the list of releases,
//! picks the newest one of the chosen channel (Stable, or Pre-release which adds the beta and alpha
//! versions) that is newer than this one, and offers it.
//!
//! - An AppImage and the Windows installer update themselves: LID downloads the file named for this system
//!   in that release's `latest.json`, checks its minisign signature against the public key built into LID
//!   ([`PUBLIC_KEY`]), and only then replaces itself (AppImage) or runs the installer (Windows). A file that
//!   is not signed with LID's key is refused.
//! - A deb or Arch install belongs to the package manager: LID only says that a new version is out and
//!   opens its release page.
//!
//! The release workflow writes `latest.json` (the format of the Tauri build's updater, so the releases of
//! both builds update each other): `{"version", "platforms": {"linux-x86_64": {"url", "signature"}}}`,
//! where `signature` is the base64 of a minisign signature file.
//!
//! The choices live in `<config>/updates.json`, apart from 1.x's settings.json (1.x has no updates).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use base64::Engine;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::HttpClient;
use crate::paths::Dirs;
use crate::tools::Progress;

pub const RELEASES_API: &str = "https://api.github.com/repos/Davidaprilio/lid/releases?per_page=30";

/// LID's update key (minisign public key, base64 of its text form). Only files signed with its secret
/// key are installed.
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDY2OUZGQjlCQUFGM0YyQzkKUldUSjh2T3FtL3VmWm1pcGZBQjUyS1B5N0VpMEpKbUJFUmFSQ3IzUmttL3lFdkEvdTlMRFVvY2UK";

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
            channel: if crate::VERSION.contains('-') {
                "prerelease"
            } else {
                "stable"
            }
            .into(),
            skipped: String::new(),
        }
    }
}

fn prefs_file(dirs: &Dirs) -> PathBuf {
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// Its `latest.json`; None when the release has none.
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
            .unwrap_or("https://github.com/Davidaprilio/lid/releases")
            .into(),
        latest_json,
        can_install: false,
    })
}

/// A build in a source checkout (target/…) never checks by itself.
pub fn is_dev_build() -> bool {
    std::env::current_exe().is_ok_and(|e| {
        e.ancestors()
            .skip(1)
            .any(|a| a.file_name().is_some_and(|n| n == "target"))
    })
}

/// The file this copy of LID replaces when it updates itself: the AppImage it runs from; on Windows the
/// installed program (the installer replaces it). None for a copy a package manager installed.
pub fn self_updatable() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::current_exe().ok();
    }
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

async fn fetch(
    client: &HttpClient,
    url: &str,
    accept: Option<&str>,
    progress: Option<&Progress>,
) -> Result<Vec<u8>, String> {
    let mut headers = IndexMap::new();
    if let Some(a) = accept {
        headers.insert("Accept".to_string(), a.to_string());
    }
    let mut resp = client
        .open(url, 0, false, &headers)
        .await
        .map_err(|e| e.message())?;
    if resp.status != 200 {
        return Err(format!("{url}: HTTP {}", resp.status));
    }
    if let Some(p) = progress {
        p.total
            .store(resp.length.map_or(-1, |n| n as i64), Ordering::Relaxed);
    }
    let mut data = Vec::new();
    while let Some(chunk) = resp
        .body
        .chunk(Duration::from_secs(30))
        .await
        .map_err(|e| e.message())?
    {
        data.extend_from_slice(&chunk);
        if let Some(p) = progress {
            p.done.store(data.len() as i64, Ordering::Relaxed);
            if p.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
        }
    }
    Ok(data)
}

/// Look for a newer version on `channel` (the releases list at `api`, [`RELEASES_API`] normally).
pub async fn find(
    client: &HttpClient,
    api: &str,
    channel: &str,
    can_install: bool,
) -> Result<Option<Found>, String> {
    let data = fetch(client, api, Some("application/vnd.github+json"), None).await?;
    let releases: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    Ok(newest(&releases, channel, crate::VERSION).map(|mut f| {
        f.can_install = can_install && f.latest_json.is_some();
        f
    }))
}

/// The key of this system in `latest.json`.
pub fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        o => o,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// Check `data` against a signature from `latest.json` (base64 of a minisign signature file) and a
/// public key (base64 of a minisign public key file).
pub fn verify(data: &[u8], signature: &str, public_key: &str) -> Result<(), String> {
    let b64 = |s: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
    };
    let key = b64(public_key)
        .and_then(|k| minisign_verify::PublicKey::decode(&k).ok())
        .ok_or("LID's update key cannot be read")?;
    let sig = b64(signature)
        .and_then(|s| minisign_verify::Signature::decode(&s).ok())
        .ok_or("the update's signature cannot be read")?;
    key.verify(data, &sig, false)
        .map_err(|_| "the update is not signed with LID's key".to_string())
}

/// The update for this system, downloaded and checked: its version and the file's bytes.
pub async fn download(
    client: &HttpClient,
    latest_json: &str,
    public_key: &str,
    progress: &Progress,
) -> Result<(String, Vec<u8>), String> {
    let manifest: Value = serde_json::from_slice(&fetch(client, latest_json, None, None).await?)
        .map_err(|e| format!("latest.json: {e}"))?;
    let key = platform_key();
    let entry = &manifest["platforms"][&key];
    let (Some(url), Some(sig)) = (entry["url"].as_str(), entry["signature"].as_str()) else {
        return Err(format!("this release has no update for {key}"));
    };
    let data = fetch(client, url, None, Some(progress)).await?;
    verify(&data, sig, public_key)?;
    let version = manifest["version"].as_str().unwrap_or("").to_string();
    Ok((version, data))
}

/// Replace the AppImage at `target` with `data` (checked first: an ELF program). The new file is written
/// beside it and moved over it, so a failure leaves the old one.
pub fn install_appimage(target: &Path, data: &[u8]) -> Result<(), String> {
    if !data.starts_with(b"\x7fELF") {
        return Err("the update is not an AppImage".into());
    }
    let dir = target.parent().ok_or("no folder")?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.update"));
    let r = (|| {
        std::fs::write(&tmp, data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&tmp, target)
    })();
    if let Err(e) = r {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Cannot replace {}: {e}", target.display()));
    }
    Ok(())
}

/// Windows: save the installer and start it for an update (passive, LID starts again afterwards). LID
/// then exits so the installer can replace it.
pub fn run_installer(data: &[u8], version: &str) -> Result<(), String> {
    if !data.starts_with(b"MZ") {
        return Err("the update is not a Windows installer".into());
    }
    let path = std::env::temp_dir().join(format!("LID-{version}-setup.exe"));
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    std::process::Command::new(&path)
        .args(["/P", "/UPDATE", "/R"])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot start the installer: {e}"))
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

    #[test]
    fn prefs_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_parts(tmp.path().into(), Some(tmp.path().into()), None, None);
        assert_eq!(load(&dirs), Prefs::default());
        let p = Prefs {
            check_at_startup: false,
            channel: "stable".into(),
            skipped: "9.9.9".into(),
        };
        save(&dirs, &p).unwrap();
        assert_eq!(load(&dirs), p);
    }

    #[test]
    fn the_built_in_key_reads() {
        // A signature by another key is refused, but the key itself must decode.
        let err = verify(b"x", "bm90IGEgc2lnbmF0dXJl", PUBLIC_KEY).unwrap_err();
        assert!(err.contains("signature"), "{err}");
    }

    /// By hand, with a file signed by the release key (`lider-pack sign FILE`):
    /// LIDER_UPDATE_TEST_FILE=FILE cargo test -p lider-core a_file_signed -- --ignored
    #[test]
    #[ignore]
    fn a_file_signed_with_the_release_key() {
        let file = std::env::var("LIDER_UPDATE_TEST_FILE").expect("LIDER_UPDATE_TEST_FILE");
        let data = std::fs::read(&file).unwrap();
        let sig = std::fs::read_to_string(format!("{file}.sig")).unwrap();
        verify(&data, &sig, PUBLIC_KEY).unwrap();
        assert!(verify(b"changed", &sig, PUBLIC_KEY).is_err());
    }

    #[test]
    fn an_appimage_is_replaced_whole_or_not_at_all() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("LID.AppImage");
        std::fs::write(&target, b"\x7fELF old").unwrap();
        assert!(install_appimage(&target, b"<html>not a program").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELF old");
        install_appimage(&target, b"\x7fELF new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELF new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        assert_eq!(
            std::fs::read_dir(tmp.path()).unwrap().count(),
            1,
            "no file left"
        );
    }
}
