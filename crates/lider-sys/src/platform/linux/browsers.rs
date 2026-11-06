//! Where Linux browsers look for the native messaging host: a `<HOST_NAME>.json` in each browser's
//! NativeMessagingHosts folder, pointing at a small shell script that runs `lider --native-host`; and the
//! same files system-wide for the packages (`browser.py` in 1.x).

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::browser::{Family, HOST_NAME, Variant, export_extension, host_manifest, pretty};
#[cfg(test)]
use crate::platform::linux::Linux;
use crate::sh;

use super::env::{Launch, SysEnv};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Browser {
    pub name: &'static str,
    /// Profile folder.
    pub config: &'static str,
    pub family: Family,
    /// NativeMessagingHosts folder.
    pub hosts_dir: &'static str,
    /// Paths are relative to the config folder, or to the home folder.
    pub in_home: bool,
    /// Gecko browsers: the folder under the config folder used when their home folder does not exist
    /// (Firefox 147 and later).
    pub xdg: &'static str,
}

const fn chromium(name: &'static str, config: &'static str, hosts_dir: &'static str) -> Browser {
    Browser {
        name,
        config,
        family: Family::Chromium,
        hosts_dir,
        in_home: false,
        xdg: "",
    }
}

pub const BROWSERS: [Browser; 8] = [
    chromium(
        "Google Chrome",
        "google-chrome",
        "google-chrome/NativeMessagingHosts",
    ),
    chromium("Chromium", "chromium", "chromium/NativeMessagingHosts"),
    chromium(
        "Brave",
        "BraveSoftware/Brave-Browser",
        "BraveSoftware/Brave-Browser/NativeMessagingHosts",
    ),
    chromium(
        "Microsoft Edge",
        "microsoft-edge",
        "microsoft-edge/NativeMessagingHosts",
    ),
    chromium("Vivaldi", "vivaldi", "vivaldi/NativeMessagingHosts"),
    chromium("Opera", "opera", "opera/NativeMessagingHosts"),
    Browser {
        name: "Firefox",
        config: ".mozilla",
        family: Family::Firefox,
        hosts_dir: ".mozilla/native-messaging-hosts",
        in_home: true,
        xdg: "mozilla",
    },
    Browser {
        name: "LibreWolf",
        config: ".librewolf",
        family: Family::Firefox,
        hosts_dir: ".librewolf/native-messaging-hosts",
        in_home: true,
        xdg: "librewolf",
    },
];

/// System-wide folders used by the .deb and Arch packages.
pub const SYSTEM_HOST_DIRS: [(Family, &[&str]); 2] = [
    (
        Family::Chromium,
        &[
            "/etc/opt/chrome/native-messaging-hosts",
            "/etc/chromium/native-messaging-hosts",
            "/etc/opt/edge/native-messaging-hosts",
            "/etc/opt/brave/native-messaging-hosts",
        ],
    ),
    (
        Family::Firefox,
        &["/usr/lib/mozilla/native-messaging-hosts"],
    ),
];

fn base(b: &Browser, env: &SysEnv) -> PathBuf {
    if b.in_home {
        env.home.clone()
    } else {
        env.config_home.clone()
    }
}

/// (profile folder, NativeMessagingHosts folder). Firefox keeps `~/.mozilla` where it exists (or with
/// MOZ_LEGACY_HOME set) and otherwise uses `$XDG_CONFIG_HOME/mozilla`; `legacy` forces a side.
pub fn paths(b: &Browser, env: &SysEnv, legacy: Option<bool>) -> (PathBuf, PathBuf) {
    if !b.xdg.is_empty() {
        let legacy =
            legacy.unwrap_or_else(|| base(b, env).join(b.config).is_dir() || env.moz_legacy_home);
        if !legacy {
            let folder = env.config_home.join(b.xdg);
            let rel = Path::new(b.hosts_dir).strip_prefix(b.config).unwrap();
            return (folder.clone(), folder.join(rel));
        }
    }
    let base = base(b, env);
    (base.join(b.config), base.join(b.hosts_dir))
}

/// The program the wrapper runs: the AppImage, else this binary.
fn host_program(env: &SysEnv) -> String {
    match env.launch() {
        Launch::AppImage(a) => a,
        _ => env.current_exe.to_string_lossy().into_owned(),
    }
}

/// Shell script the browser runs; it knows how this copy of LID is started.
pub fn wrapper_script(env: &SysEnv) -> String {
    format!(
        "#!/bin/sh\n\
         # Native messaging host for the LID browser extension. Written by LID; safe to delete.\n\
         export LIDER_LAUNCH={}\n\
         exec {} --native-host \"$@\"\n",
        sh::quote(&env.launch_command(&["--minimized"])),
        sh::quote(&host_program(env)),
    )
}

pub fn host_executable(env: &SysEnv) -> PathBuf {
    env.dirs()
        .data
        .join("native-host")
        .join("lider-native-host")
}

/// The browsers with these names, in table order.
pub fn by_name(names: &[&str]) -> Vec<Browser> {
    BROWSERS
        .iter()
        .filter(|b| names.contains(&b.name))
        .copied()
        .collect()
}

pub fn detected(env: &SysEnv) -> Vec<Browser> {
    BROWSERS
        .iter()
        .filter(|b| paths(b, env, None).0.is_dir())
        .copied()
        .collect()
}

pub fn is_installed(b: &Browser, env: &SysEnv) -> bool {
    paths(b, env, None)
        .1
        .join(format!("{HOST_NAME}.json"))
        .is_file()
}

/// Write the wrapper once and a host manifest for every browser. Returns the wrapper path.
pub fn install(browsers: &[Browser], env: &SysEnv) -> io::Result<PathBuf> {
    let exe = host_executable(env);
    fs::create_dir_all(exe.parent().unwrap())?;
    fs::write(&exe, wrapper_script(env))?;
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755))?;
    for b in browsers {
        let folder = paths(b, env, None).1;
        fs::create_dir_all(&folder)?;
        fs::write(
            folder.join(format!("{HOST_NAME}.json")),
            pretty(&host_manifest(b.family, &exe.to_string_lossy())),
        )?;
    }
    Ok(exe)
}

pub fn uninstall(browsers: &[Browser], env: &SysEnv) {
    for b in browsers {
        let sides: &[Option<bool>] = if b.xdg.is_empty() {
            &[None]
        } else {
            &[Some(true), Some(false)]
        };
        for legacy in sides {
            let _ = fs::remove_file(paths(b, env, *legacy).1.join(format!("{HOST_NAME}.json")));
        }
    }
}

/// Exported copy in the user's data folder; browsers load it as an unpacked extension.
pub fn extension_folder(family: Family, env: &SysEnv) -> io::Result<PathBuf> {
    export_extension(
        family,
        &env.dirs()
            .data
            .join("browser-extension")
            .join(family.as_str()),
        Variant::Full,
        "",
    )
}

/// For the package builds: host script, system-wide host manifests, exported extensions under `root`.
pub fn write_system_files(root: &Path, host_path: &str, lider: &str) -> io::Result<()> {
    let exe = root.join(host_path.trim_start_matches('/'));
    fs::create_dir_all(exe.parent().unwrap())?;
    fs::write(
        &exe,
        format!(
            "#!/bin/sh\n# Native messaging host for the LID browser extension.\nexec {} --native-host \"$@\"\n",
            sh::quote(lider)
        ),
    )?;
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755))?;
    for (family, folders) in SYSTEM_HOST_DIRS {
        for folder in folders {
            let target = root
                .join(folder.trim_start_matches('/'))
                .join(format!("{HOST_NAME}.json"));
            fs::create_dir_all(target.parent().unwrap())?;
            fs::write(&target, pretty(&host_manifest(family, host_path)))?;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o644))?;
        }
    }
    for family in [Family::Chromium, Family::Firefox] {
        export_extension(
            family,
            &root
                .join("usr/share/lider/browser-extension")
                .join(family.as_str()),
            Variant::Full,
            "",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Port of tests/test_browser.py (the Linux side).
    use super::*;
    use crate::browser::FIREFOX_EXTENSION_ID;
    use serde_json::{Value, json};

    fn home() -> (tempfile::TempDir, SysEnv) {
        let tmp = tempfile::tempdir().unwrap();
        let env = SysEnv::for_test(tmp.path(), None);
        fs::create_dir_all(env.config_home.join("BraveSoftware/Brave-Browser")).unwrap();
        fs::create_dir_all(tmp.path().join(".mozilla")).unwrap();
        (tmp, env)
    }

    #[test]
    fn detect_install_uninstall() {
        let (tmp, env) = home();
        let found = detected(&env);
        assert_eq!(
            found.iter().map(|b| b.name).collect::<Vec<_>>(),
            ["Brave", "Firefox"]
        );
        let exe = install(&found, &env).unwrap();
        assert!(fs::metadata(&exe).unwrap().permissions().mode() & 0o100 != 0);
        assert!(
            fs::read_to_string(&exe)
                .unwrap()
                .contains("--native-host \"$@\"")
        );
        let brave: Value = serde_json::from_str(
            &fs::read_to_string(env.config_home.join(format!(
                "BraveSoftware/Brave-Browser/NativeMessagingHosts/{HOST_NAME}.json"
            )))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            brave["allowed_origins"],
            host_manifest(Family::Chromium, "")["allowed_origins"]
        );
        assert_eq!(brave["path"], exe.to_string_lossy().as_ref());
        let fox: Value = serde_json::from_str(
            &fs::read_to_string(
                tmp.path()
                    .join(format!(".mozilla/native-messaging-hosts/{HOST_NAME}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(fox["allowed_extensions"], json!([FIREFOX_EXTENSION_ID]));
        assert!(found.iter().all(|b| is_installed(b, &env)));
        uninstall(&found, &env);
        assert!(!found.iter().any(|b| is_installed(b, &env)));
    }

    #[test]
    fn firefox_in_the_xdg_config_folder() {
        let (tmp, env) = home();
        fs::remove_dir(tmp.path().join(".mozilla")).unwrap();
        fs::create_dir_all(env.config_home.join("mozilla/firefox")).unwrap();
        let found = detected(&env);
        let fox = *found.iter().find(|b| b.name == "Firefox").unwrap();
        install(&[fox], &env).unwrap();
        assert!(
            env.config_home
                .join(format!("mozilla/native-messaging-hosts/{HOST_NAME}.json"))
                .is_file()
        );
        assert!(!tmp.path().join(".mozilla").exists());
        assert!(is_installed(&fox, &env));
        uninstall(&[fox], &env);
        assert!(!is_installed(&fox, &env));
    }

    #[test]
    fn appimage_wrapper_runs_the_appimage() {
        let tmp = tempfile::tempdir().unwrap();
        let env = SysEnv::for_test(tmp.path(), Some("/opt/apps/Lider 1.0.AppImage"));
        let script = wrapper_script(&env);
        assert!(
            script.contains("exec '/opt/apps/Lider 1.0.AppImage' --native-host"),
            "{script}"
        );
        assert!(
            script.contains(
                "export LIDER_LAUNCH=''\"'\"'/opt/apps/Lider 1.0.AppImage'\"'\"' --minimized'"
            ),
            "{script}"
        );
    }

    #[test]
    fn system_files_for_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        write_system_files(&root, "/usr/lib/lider/lider-native-host", "/usr/bin/lider").unwrap();
        let host = fs::read_to_string(root.join("usr/lib/lider/lider-native-host")).unwrap();
        assert!(host.contains("exec /usr/bin/lider --native-host \"$@\""));
        let chrome: Value = serde_json::from_str(
            &fs::read_to_string(root.join(format!(
                "etc/opt/chrome/native-messaging-hosts/{HOST_NAME}.json"
            )))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(chrome["path"], "/usr/lib/lider/lider-native-host");
        assert!(
            root.join(format!(
                "usr/lib/mozilla/native-messaging-hosts/{HOST_NAME}.json"
            ))
            .is_file()
        );
        assert!(
            root.join("usr/share/lider/browser-extension/chromium/manifest.json")
                .is_file()
        );
    }

    #[test]
    fn cli_messages() {
        let tmp = tempfile::tempdir().unwrap();
        let env = SysEnv::for_test(tmp.path(), None);
        let os = Linux::new(env);
        assert_eq!(
            crate::browser::main(&["--install-browser-integration".into()], &os),
            1
        );
        assert_eq!(
            crate::browser::main(&["--uninstall-browser-integration".into()], &os),
            0
        );
    }
}
