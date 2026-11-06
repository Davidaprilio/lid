//! Getting yt-dlp when YouTube needs it: the standalone program for this system from yt-dlp's own
//! releases on GitHub, checked against the `SHA2-256SUMS` of the same release, kept in LID's data folder
//! (`<data>/tools/`). Nothing is downloaded unless the user agrees in the window that asks.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use indexmap::IndexMap;
use sha2::{Digest, Sha256};

use crate::http::HttpClient;
use crate::paths::Dirs;

/// The latest release of yt-dlp (its project page, not a mirror).
pub const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
pub const PROJECT_URL: &str = "https://github.com/yt-dlp/yt-dlp";

/// The release file for this system, if yt-dlp publishes one.
pub fn yt_dlp_asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") if cfg!(target_env = "musl") => Some("yt-dlp_musllinux"),
        ("linux", "x86_64") => Some("yt-dlp_linux"),
        ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
        ("windows", "x86_64") => Some("yt-dlp.exe"),
        ("windows", "x86") => Some("yt-dlp_x86.exe"),
        ("windows", "aarch64") => Some("yt-dlp_arm64.exe"),
        ("macos", _) => Some("yt-dlp_macos"),
        _ => None,
    }
}

/// Where LID keeps the copy it downloaded.
pub fn yt_dlp_path(dirs: &Dirs) -> PathBuf {
    dirs.data.join("tools").join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    })
}

/// How far a download is, for the progress bar. `total` is -1 while unknown.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicI64,
    pub total: AtomicI64,
    pub cancel: AtomicBool,
    pub error: Mutex<String>,
}

/// The expected SHA-256 of `asset` in a `SHA2-256SUMS` file (`<hex>  <name>` lines).
pub fn sum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let hex = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset && hex.len() == 64).then(|| hex.to_ascii_lowercase())
    })
}

async fn fetch(
    client: &HttpClient,
    url: &str,
    progress: Option<&Progress>,
) -> Result<Vec<u8>, String> {
    let mut resp = client
        .open(url, 0, false, &IndexMap::new())
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

/// Download yt-dlp for this system into `yt_dlp_path(dirs)`. The file is only kept when its SHA-256 is
/// the one the release lists.
pub async fn install_yt_dlp(
    client: &HttpClient,
    dirs: &Dirs,
    progress: &Progress,
) -> Result<PathBuf, String> {
    let asset = yt_dlp_asset().ok_or("yt-dlp publishes no program for this system")?;
    let sums = fetch(client, &format!("{RELEASES}/SHA2-256SUMS"), None).await?;
    let want = sum_for(&String::from_utf8_lossy(&sums), asset)
        .ok_or("the release lists no checksum for this system")?;
    let data = fetch(client, &format!("{RELEASES}/{asset}"), Some(progress)).await?;
    let got = format!("{:x}", Sha256::digest(&data));
    if got != want {
        return Err(format!(
            "the downloaded file does not match its checksum (SHA-256 {got}, expected {want})"
        ));
    }
    let target = yt_dlp_path(dirs);
    let dir = target.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{asset}.part"));
    std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    crate::media::forget_yt_dlp();
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_line_for_the_asset() {
        let sums = "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a  yt-dlp.exe\n\
                    58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a  yt-dlp_linux\n\
                    32e72032766bef9199d99d15beb69fd52e46df8f8b06f0d8745db59e04d339e9  yt-dlp_linux.zip\n";
        assert_eq!(
            sum_for(sums, "yt-dlp_linux").unwrap(),
            "58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a"
        );
        assert!(sum_for(sums, "yt-dlp_macos").is_none());
        assert!(
            sum_for(sums, "yt-dlp").is_none(),
            "names must match exactly"
        );
    }

    #[test]
    fn the_copy_lives_in_the_data_folder() {
        let d = Dirs::from_parts("/h".into(), None, Some("/d".into()), None);
        assert!(yt_dlp_path(&d).starts_with("/d/lider/tools"));
    }
}

/// The real download (LIDER_NETWORK_TESTS=1): yt-dlp for this system from its releases, checked, runnable.
#[cfg(test)]
mod network {
    use super::*;

    #[tokio::test]
    async fn downloads_yt_dlp_for_this_system() {
        if std::env::var("LIDER_NETWORK_TESTS").is_err() {
            eprintln!("skipped: set LIDER_NETWORK_TESTS=1 to download yt-dlp");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let dirs = Dirs::from_parts(
            tmp.path().into(),
            None,
            Some(tmp.path().join("d").into()),
            None,
        );
        let client = HttpClient::new("LID test", "", 30.0).unwrap();
        let p = Progress::default();
        let path = install_yt_dlp(&client, &dirs, &p).await.unwrap();
        assert_eq!(path, yt_dlp_path(&dirs));
        assert!(p.done.load(Ordering::Relaxed) > 1_000_000);
        let out = std::process::Command::new(&path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(out.status.success());
        println!("yt-dlp {}", String::from_utf8_lossy(&out.stdout).trim());
    }
}
