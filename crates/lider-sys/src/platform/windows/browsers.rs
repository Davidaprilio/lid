//! Where Windows browsers find the native messaging host: a manifest file anywhere, named by the default
//! value of `HKCU\<browser key>\NativeMessagingHosts\<HOST_NAME>`. The manifest points straight at
//! lider.exe; the browser's own arguments (`chrome-extension://…`, or Firefox's manifest path and add-on id)
//! tell it to run as the host (`native_host::started_by_browser`).
//!
//! Only the registry locations the browsers document are used (as on Linux, decision Q8): Chrome,
//! Chromium, Edge, Brave and Firefox. Opera and Vivaldi read Chrome's key.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

use crate::browser::{Family, HOST_NAME, Variant, export_extension, host_manifest, pretty};
use crate::platform::BrowserState;

use super::Windows;

pub struct Browser {
    pub name: &'static str,
    pub family: Family,
    /// Its profile folder, under `%LOCALAPPDATA%` (Chromium family) or `%APPDATA%` (Firefox).
    pub profile: &'static str,
    /// Its registry key under `HKCU\Software`.
    pub key: &'static str,
}

pub const BROWSERS: [Browser; 5] = [
    Browser {
        name: "Google Chrome",
        family: Family::Chromium,
        profile: r"Google\Chrome\User Data",
        key: r"Google\Chrome",
    },
    Browser {
        name: "Chromium",
        family: Family::Chromium,
        profile: r"Chromium\User Data",
        key: "Chromium",
    },
    Browser {
        name: "Brave",
        family: Family::Chromium,
        profile: r"BraveSoftware\Brave-Browser\User Data",
        key: r"BraveSoftware\Brave-Browser",
    },
    Browser {
        name: "Microsoft Edge",
        family: Family::Chromium,
        profile: r"Microsoft\Edge\User Data",
        key: r"Microsoft\Edge",
    },
    Browser {
        name: "Firefox",
        family: Family::Firefox,
        profile: r"Mozilla\Firefox",
        key: "Mozilla",
    },
];

impl Windows {
    fn hosts_key(&self, b: &Browser) -> String {
        format!(
            r"{}\{}\NativeMessagingHosts\{HOST_NAME}",
            self.registry_root, b.key
        )
    }

    fn profile_dir(&self, b: &Browser) -> PathBuf {
        match b.family {
            Family::Chromium => self.local_appdata.join(b.profile),
            Family::Firefox => self.appdata.join(b.profile),
        }
    }

    /// One manifest per family, next to LID's data.
    pub fn manifest_path(&self, family: Family) -> PathBuf {
        self.dirs()
            .data
            .join("native-host")
            .join(format!("{HOST_NAME}.{}.json", family.as_str()))
    }

    fn registered(&self, b: &Browser) -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(self.hosts_key(b))
            .ok()?
            .get_value::<String, _>("")
            .ok()
    }

    pub fn states(&self) -> Vec<BrowserState> {
        BROWSERS
            .iter()
            .map(|b| BrowserState {
                name: b.name,
                family: b.family,
                detected: self.profile_dir(b).is_dir(),
                installed: self.registered(b).is_some_and(|m| Path::new(&m).is_file()),
            })
            .collect()
    }

    pub fn install_hosts(&self, names: &[&str]) -> io::Result<PathBuf> {
        let exe = self.current_exe.clone();
        for b in BROWSERS.iter().filter(|b| names.contains(&b.name)) {
            let manifest = self.manifest_path(b.family);
            fs::create_dir_all(manifest.parent().unwrap())?;
            fs::write(
                &manifest,
                pretty(&host_manifest(b.family, &exe.to_string_lossy())),
            )?;
            let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(self.hosts_key(b))?;
            key.set_value("", &manifest.to_string_lossy().into_owned())?;
        }
        Ok(exe)
    }

    pub fn uninstall_hosts(&self, names: &[&str]) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for b in BROWSERS.iter().filter(|b| names.contains(&b.name)) {
            let _ = hkcu.delete_subkey_all(self.hosts_key(b));
        }
    }

    pub fn export(&self, family: Family) -> io::Result<PathBuf> {
        export_extension(family, &self.extension_dir(family), Variant::Full, "")
    }

    pub fn extension_dir(&self, family: Family) -> PathBuf {
        self.dirs()
            .data
            .join("browser-extension")
            .join(family.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::BrowserHosts;

    #[test]
    fn register_and_remove_in_the_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let os = Windows::for_test(
            tmp.path(),
            &format!(r"Software\lider-test-{}", std::process::id()),
        );
        fs::create_dir_all(
            tmp.path()
                .join(r"Local\BraveSoftware\Brave-Browser\User Data"),
        )
        .unwrap();
        fs::create_dir_all(tmp.path().join(r"Roaming\Mozilla\Firefox")).unwrap();
        let found: Vec<&str> = os
            .browsers()
            .iter()
            .filter(|b| b.detected)
            .map(|b| b.name)
            .collect();
        assert_eq!(found, ["Brave", "Firefox"]);
        let exe = os.install(&found).unwrap();
        assert!(
            os.browsers()
                .iter()
                .filter(|b| b.detected)
                .all(|b| b.installed)
        );
        let fox: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(os.manifest_path(Family::Firefox)).unwrap())
                .unwrap();
        assert_eq!(fox["path"], exe.to_string_lossy().as_ref());
        assert_eq!(
            fox["allowed_extensions"][0],
            crate::browser::FIREFOX_EXTENSION_ID
        );
        os.uninstall(&found);
        assert!(!os.browsers().iter().any(|b| b.installed));
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&os.registry_root);
    }
}
