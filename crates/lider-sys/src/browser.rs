//! Browser integration, the part that is the same on every OS (`browser.py`): the host manifest, the
//! extension built into the program and exported per browser family, the release files, and the
//! `--install-browser-integration` command. Where a browser looks for the host (files on Linux, the
//! registry on Windows) is the platform's [`BrowserHosts`] port.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};
use serde_json::{Value, json};

use crate::platform::BrowserHosts;

pub const HOST_NAME: &str = "io.github.davidaprilio.lid";
/// The extension's own version, apart from LID's: raise it when the extension changes (stores and
/// Mozilla's signing refuse a version twice).
pub const EXTENSION_VERSION: &str = "1.0.0";
/// The id fixed by the "key" in manifest.chromium.json (unpacked installs and the GitHub zip).
pub const CHROMIUM_EXTENSION_ID: &str = "hljibhdjbmpphbjhbfgiejnijpeobbne";
/// The id Microsoft Edge Add-ons gave the store build.
pub const EDGE_ADDONS_EXTENSION_ID: &str = "ophlhhmpoaonhglimkemkgjifigpghej";
/// Every Chromium extension id the helper accepts.
pub const CHROMIUM_EXTENSION_IDS: [&str; 2] = [CHROMIUM_EXTENSION_ID, EDGE_ADDONS_EXTENSION_ID];
pub const FIREFOX_EXTENSION_ID: &str = "lid@davidaprilio.github.io";
pub const DESCRIPTION: &str = "LID (Linux Internet Downloader) browser integration";

pub const RELEASES_URL: &str = "https://github.com/Davidaprilio/lid/releases/latest";
/// Store pages; empty until the listings are published.
pub const CHROME_WEB_STORE_URL: &str = "";
pub const FIREFOX_ADDONS_URL: &str = "";
/// Releases carry the signed .xpi only when CI has the AMO keys.
pub const FIREFOX_SIGNED_XPI: bool = false;
pub const FIREFOX_XPI_URL: &str =
    "https://github.com/Davidaprilio/lid/releases/latest/download/lid-extension-firefox.xpi";
pub const FIREFOX_ZIP_URL: &str =
    "https://github.com/Davidaprilio/lid/releases/latest/download/lid-extension-firefox.zip";
pub const FIREFOX_UPDATES_URL: &str =
    "https://github.com/Davidaprilio/lid/releases/latest/download/firefox-updates.json";
pub const CHROMIUM_ZIP_URL: &str =
    "https://github.com/Davidaprilio/lid/releases/latest/download/lid-extension-chromium.zip";

/// Chrome Web Store does not accept extensions that download from YouTube, and "key" is not allowed in
/// uploads: the "store" variant leaves both out.
pub const STORE_DESCRIPTION: &str =
    "Send downloads and links from the browser to LID (Linux Internet Downloader).";
pub const YOUTUBE_FILES: [&str; 2] = ["youtube.js", "youtube.css"];

/// The extension sources, built into the program.
pub static EXTENSION: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../lider/browser_extension");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Chromium,
    Firefox,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Chromium => "chromium",
            Family::Firefox => "firefox",
        }
    }
}

pub fn host_manifest(family: Family, host_path: &str) -> Value {
    let mut m =
        json!({"name": HOST_NAME, "description": DESCRIPTION, "path": host_path, "type": "stdio"});
    match family {
        Family::Firefox => m["allowed_extensions"] = json!([FIREFOX_EXTENSION_ID]),
        Family::Chromium => {
            m["allowed_origins"] = json!(
                CHROMIUM_EXTENSION_IDS
                    .iter()
                    .map(|i| format!("chrome-extension://{i}/"))
                    .collect::<Vec<_>>()
            )
        }
    }
    m
}

pub(crate) fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap() + "\n"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Full,
    /// No YouTube button and no manifest key (Chrome Web Store rules).
    Store,
}

fn write_tree(dir: &Dir<'_>, dest: &Path, skip: &dyn Fn(&str) -> bool) -> io::Result<()> {
    for f in dir.files() {
        let rel = f.path().to_string_lossy();
        let name = f.path().file_name().unwrap().to_string_lossy();
        if skip(&name) {
            continue;
        }
        let target = dest.join(&*rel);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(target, f.contents())?;
    }
    for d in dir.dirs() {
        write_tree(d, dest, skip)?;
    }
    Ok(())
}

/// Copy the extension for one browser family into `dest` with its manifest.json. `update_url`
/// (Firefox, self-distributed .xpi) lets Firefox find newer signed versions.
pub fn export_extension(
    family: Family,
    dest: &Path,
    variant: Variant,
    update_url: &str,
) -> io::Result<PathBuf> {
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    let skip = |name: &str| {
        (name.starts_with("manifest.") && name.ends_with(".json") && name != "manifest.json")
            || name.ends_with(".py")
            || (variant == Variant::Store && YOUTUBE_FILES.contains(&name))
    };
    write_tree(&EXTENSION, dest, &skip)?;
    let source = EXTENSION
        .get_file(format!("manifest.{}.json", family.as_str()))
        .expect("both manifests are built in");
    let mut manifest: Value =
        serde_json::from_slice(source.contents()).map_err(io::Error::other)?;
    let m = manifest.as_object_mut().unwrap();
    m.insert("version".into(), EXTENSION_VERSION.into());
    if variant == Variant::Store {
        m.shift_remove("key");
        m.shift_remove("content_scripts");
        m.insert("description".into(), STORE_DESCRIPTION.into());
    }
    if !update_url.is_empty() && family == Family::Firefox {
        manifest["browser_specific_settings"]["gecko"]["update_url"] = update_url.into();
    }
    fs::write(dest.join("manifest.json"), pretty(&manifest))?;
    Ok(dest.to_path_buf())
}

/// Reproducible zip: sorted entries, fixed timestamps and permissions.
pub fn zip_folder(folder: &Path, out: &Path) -> io::Result<PathBuf> {
    fn walk(dir: &Path, all: &mut Vec<PathBuf>) -> io::Result<()> {
        for e in fs::read_dir(dir)? {
            let p = e?.path();
            if p.is_dir() {
                walk(&p, all)?
            } else {
                all.push(p)
            }
        }
        Ok(())
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut files = Vec::new();
    walk(folder, &mut files)?;
    let mut names: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|p| {
            (
                p.strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                p,
            )
        })
        .collect();
    names.sort();
    let mut z = zip::ZipWriter::new(fs::File::create(out)?);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for (name, path) in names {
        z.start_file(name, opts).map_err(io::Error::other)?;
        z.write_all(&fs::read(path)?)?;
    }
    z.finish().map_err(io::Error::other)?;
    Ok(out.to_path_buf())
}

/// The update manifest Firefox reads from gecko.update_url.
pub fn firefox_updates_json(version: &str, xpi_url: &str) -> Value {
    json!({"addons": {FIREFOX_EXTENSION_ID: {"updates": [{"version": version, "update_link": xpi_url}]}}})
}

/// Release files for `./packaging/build.sh extension`:
/// - `dist/lid-extension-chromium.zip`: unpacked install, fixed id (has the key)
/// - `dist/lid-extension-chromium-store.zip`: Chrome Web Store / Edge Add-ons upload
/// - `dist/lid-extension-firefox.zip`: addons.mozilla.org upload / temporary loading
/// - `work/firefox-unlisted/`: source for `web-ext sign --channel unlisted`
/// - `dist/firefox-updates.json`: update manifest for the self-distributed .xpi
pub fn build_extensions(dist: &Path, work: &Path) -> io::Result<()> {
    zip_folder(
        &export_extension(Family::Chromium, &work.join("chromium"), Variant::Full, "")?,
        &dist.join("lid-extension-chromium.zip"),
    )?;
    zip_folder(
        &export_extension(
            Family::Chromium,
            &work.join("chromium-store"),
            Variant::Store,
            "",
        )?,
        &dist.join("lid-extension-chromium-store.zip"),
    )?;
    zip_folder(
        &export_extension(Family::Firefox, &work.join("firefox"), Variant::Full, "")?,
        &dist.join("lid-extension-firefox.zip"),
    )?;
    export_extension(
        Family::Firefox,
        &work.join("firefox-unlisted"),
        Variant::Full,
        FIREFOX_UPDATES_URL,
    )?;
    let xpi = format!(
        "https://github.com/Davidaprilio/lid/releases/download/v{}/lid-extension-firefox.xpi",
        lider_core::VERSION
    );
    fs::write(
        dist.join("firefox-updates.json"),
        pretty(&firefox_updates_json(EXTENSION_VERSION, &xpi)),
    )
}

/// `lider --install-browser-integration` / `--uninstall-browser-integration`.
pub fn main(argv: &[String], os: &impl BrowserHosts) -> i32 {
    let all: Vec<&str> = os.browsers().iter().map(|b| b.name).collect();
    if argv.iter().any(|a| a == "--uninstall-browser-integration") {
        os.uninstall(&all);
        println!("Removed the LID native messaging host from every browser.");
        return 0;
    }
    let found: Vec<_> = os.browsers().into_iter().filter(|b| b.detected).collect();
    if found.is_empty() {
        println!(
            "No supported browser profile found ({}).",
            os.browser_list_text()
        );
        return 1;
    }
    let names: Vec<&str> = found.iter().map(|b| b.name).collect();
    let exe = match os.install(&names) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Cannot register the native messaging host: {e}");
            return 1;
        }
    };
    println!("Native messaging host: {}", exe.display());
    for b in &found {
        println!("  registered for {}", b.name);
    }
    let mut families: Vec<Family> = found.iter().map(|b| b.family).collect();
    families.sort_by_key(|f| f.as_str());
    families.dedup();
    for family in families {
        match os.extension_folder(family) {
            Ok(f) => println!("Extension for {}: {}", family.as_str(), f.display()),
            Err(e) => eprintln!("Cannot export the extension for {}: {e}", family.as_str()),
        }
    }
    println!(
        "Load that folder as an unpacked extension (see Options -> General -> Browser integration)."
    );
    0
}

#[cfg(test)]
mod tests {
    //! Port of tests/test_browser.py.
    use super::*;

    fn manifest_of(family: &str) -> Value {
        serde_json::from_slice(
            EXTENSION
                .get_file(format!("manifest.{family}.json"))
                .unwrap()
                .contents(),
        )
        .unwrap()
    }

    #[test]
    fn chromium_key_gives_the_registered_extension_id() {
        use base64_like::decode;
        let key = manifest_of("chromium")["key"].as_str().unwrap().to_string();
        let digest = sha256_hex(&decode(&key));
        let id: String = digest[..32]
            .chars()
            .map(|c| (b'a' + c.to_digit(16).unwrap() as u8) as char)
            .collect();
        assert_eq!(id, CHROMIUM_EXTENSION_ID);
    }

    fn sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(data))
    }

    mod base64_like {
        pub fn decode(s: &str) -> Vec<u8> {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.decode(s).unwrap()
        }
    }

    #[test]
    fn firefox_id_matches() {
        assert_eq!(
            manifest_of("firefox")["browser_specific_settings"]["gecko"]["id"],
            FIREFOX_EXTENSION_ID
        );
    }

    #[test]
    fn manifests_reference_existing_files() {
        for family in ["chromium", "firefox"] {
            let m = manifest_of(family);
            let mut files: Vec<String> = m["icons"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_str().unwrap().into())
                .collect();
            files.push(m["action"]["default_popup"].as_str().unwrap().into());
            if let Some(s) = m["background"].get("scripts").and_then(Value::as_array) {
                files.extend(s.iter().map(|v| v.as_str().unwrap().into()));
            }
            files.push(
                m["background"]
                    .get("service_worker")
                    .and_then(Value::as_str)
                    .unwrap_or("background.js")
                    .into(),
            );
            for cs in m["content_scripts"].as_array().unwrap() {
                files.extend(
                    cs["js"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().into()),
                );
                if let Some(css) = cs.get("css").and_then(Value::as_array) {
                    files.extend(css.iter().map(|v| v.as_str().unwrap().into()));
                }
            }
            for f in files {
                assert!(EXTENSION.get_file(&f).is_some(), "{family}: {f}");
            }
            assert!(
                m["permissions"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("nativeMessaging"))
            );
        }
    }

    #[test]
    fn background_uses_the_same_host_name() {
        let bg = EXTENSION
            .get_file("background.js")
            .unwrap()
            .contents_utf8()
            .unwrap();
        assert!(bg.contains(&format!("\"{HOST_NAME}\"")));
    }

    fn read(p: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(p.join("manifest.json")).unwrap()).unwrap()
    }

    #[test]
    fn store_variant_has_no_youtube_and_no_key() {
        let tmp = tempfile::tempdir().unwrap();
        let full = read(
            &export_extension(
                Family::Chromium,
                &tmp.path().join("full"),
                Variant::Full,
                "",
            )
            .unwrap(),
        );
        let store_dir = export_extension(
            Family::Chromium,
            &tmp.path().join("store"),
            Variant::Store,
            "",
        )
        .unwrap();
        let store = read(&store_dir);
        assert!(full.get("key").is_some() && full.get("content_scripts").is_some());
        assert!(store.get("key").is_none() && store.get("content_scripts").is_none());
        assert!(!store_dir.join("youtube.js").exists());
        assert_eq!(store["version"], EXTENSION_VERSION);
        assert!(!store["description"].as_str().unwrap().contains("YouTube"));
        assert!(tmp.path().join("full/icons/128.png").is_file());
        assert!(!tmp.path().join("full/manifest.chromium.json").exists());
    }

    #[test]
    fn update_url_only_for_the_self_distributed_firefox_build() {
        let tmp = tempfile::tempdir().unwrap();
        let listed = read(
            &export_extension(Family::Firefox, &tmp.path().join("l"), Variant::Full, "").unwrap(),
        );
        let unlisted = read(
            &export_extension(
                Family::Firefox,
                &tmp.path().join("u"),
                Variant::Full,
                FIREFOX_UPDATES_URL,
            )
            .unwrap(),
        );
        assert!(
            listed["browser_specific_settings"]["gecko"]
                .get("update_url")
                .is_none()
        );
        assert_eq!(
            unlisted["browser_specific_settings"]["gecko"]["update_url"],
            FIREFOX_UPDATES_URL
        );
    }

    #[test]
    fn release_files_are_reproducible() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        build_extensions(&t.join("a"), &t.join("wa")).unwrap();
        build_extensions(&t.join("b"), &t.join("wb")).unwrap();
        let mut names: Vec<String> = fs::read_dir(t.join("a"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "firefox-updates.json",
                "lid-extension-chromium-store.zip",
                "lid-extension-chromium.zip",
                "lid-extension-firefox.zip"
            ]
        );
        for n in &names {
            assert_eq!(
                fs::read(t.join("a").join(n)).unwrap(),
                fs::read(t.join("b").join(n)).unwrap(),
                "{n}"
            );
        }
        let z = zip::ZipArchive::new(
            fs::File::open(t.join("a/lid-extension-chromium-store.zip")).unwrap(),
        )
        .unwrap();
        let entries: Vec<&str> = z.file_names().collect();
        assert!(entries.contains(&"manifest.json"));
        assert!(!entries.contains(&"youtube.js"));
        let updates: Value =
            serde_json::from_str(&fs::read_to_string(t.join("a/firefox-updates.json")).unwrap())
                .unwrap();
        let entry = &updates["addons"][FIREFOX_EXTENSION_ID]["updates"][0];
        assert_eq!(entry["version"], EXTENSION_VERSION);
        assert!(entry["update_link"].as_str().unwrap().ends_with(&format!(
            "/v{}/lid-extension-firefox.xpi",
            lider_core::VERSION
        )));
    }

    #[test]
    fn host_accepts_every_known_chromium_id() {
        let origins = host_manifest(Family::Chromium, "/x")["allowed_origins"].clone();
        assert_eq!(
            origins,
            json!([
                format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/"),
                format!("chrome-extension://{EDGE_ADDONS_EXTENSION_ID}/")
            ])
        );
    }

    #[test]
    fn export_extension_per_family() {
        let tmp = tempfile::tempdir().unwrap();
        let dest =
            export_extension(Family::Firefox, &tmp.path().join("ext"), Variant::Full, "").unwrap();
        assert_eq!(
            read(&dest)["browser_specific_settings"]["gecko"]["id"],
            FIREFOX_EXTENSION_ID
        );
        assert!(!dest.join("manifest.firefox.json").exists());
        assert!(dest.join("background.js").is_file());
    }
}
