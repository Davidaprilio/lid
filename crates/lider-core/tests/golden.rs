//! The 1.2 profile in tests/golden/v1.2 (written by the Python code) must read back exactly as 1.2
//! reads it (expected.json), and what 2.x writes must read back the same.

use std::fs;
use std::path::{Path, PathBuf};

use lider_core::paths::Dirs;
use lider_core::settings::{self, Settings};
use lider_core::store::Store;
use serde_json::{Map, Value};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/v1.2")
}

fn expected() -> Value {
    serde_json::from_str(&fs::read_to_string(golden().join("expected.json")).unwrap()).unwrap()
}

/// The folders make_golden.py used: config and data inside the golden folder, a fixed home and cache.
fn dirs(root: &Path) -> Dirs {
    Dirs::from_parts(
        "/home/lid-user".into(),
        Some(root.join("config").into()),
        Some(root.join("data").into()),
        Some("/home/lid-user/.cache".into()),
    )
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}

fn queues_json(store: &Store) -> Value {
    let (queues, bad) = store.load_queues().unwrap();
    assert!(bad.is_empty(), "{bad:?}");
    Value::Object(
        queues
            .into_iter()
            .map(|(k, v)| (k, serde_json::to_value(v).unwrap()))
            .collect::<Map<_, _>>(),
    )
}

fn downloads_json(store: &Store) -> Value {
    let (downloads, bad) = store.load_downloads().unwrap();
    assert!(bad.is_empty(), "{bad:?}");
    Value::Array(downloads.iter().map(|d| d.to_json()).collect())
}

#[test]
fn reads_the_1_2_profile_like_1_2() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&golden(), tmp.path());
    let dirs = dirs(tmp.path());
    let want = expected();

    assert_eq!(settings::load(&dirs).to_json(), want["settings"]);
    let legacy = settings::load_from(&tmp.path().join("config/settings-legacy.json"), &dirs);
    assert_eq!(legacy.to_json(), want["settings_legacy"]);
    let missing = settings::load_from(&tmp.path().join("config/absent.json"), &dirs);
    assert_eq!(missing.to_json(), want["settings_missing"]);

    let store = Store::open(&dirs.database_file()).unwrap();
    assert_eq!(downloads_json(&store), want["downloads"]);
    assert_eq!(queues_json(&store), want["queues"]);
    assert_eq!(store.next_id().unwrap(), want["next_id"].as_i64().unwrap());
}

/// What 2.x writes reads back unchanged. With LIDER_GOLDEN_OUT set, the written profile is also left
/// in that folder for tests/golden/check_rust_profile.py, which reads it with the 1.2 code.
#[test]
fn writes_what_it_reads() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&golden(), tmp.path());
    let src = dirs(tmp.path());
    let out_root = std::env::var_os("LIDER_GOLDEN_OUT").map(PathBuf::from);
    let out_tmp = tempfile::tempdir().unwrap();
    let out_root = out_root.unwrap_or_else(|| out_tmp.path().to_path_buf());
    let _ = fs::remove_dir_all(&out_root);
    let out = dirs(&out_root);

    let s: Settings = settings::load(&src);
    settings::save(&s, &out).unwrap();
    let legacy = settings::load_from(&tmp.path().join("config/settings-legacy.json"), &src);
    settings::save_to(&legacy, &out_root.join("config/settings-legacy.json")).unwrap();
    assert_eq!(settings::load(&out), s);

    let a = Store::open(&src.database_file()).unwrap();
    let b = Store::open(&out.database_file()).unwrap();
    let (downloads, _) = a.load_downloads().unwrap();
    b.save_downloads(&downloads).unwrap();
    let (queues, _) = a.load_queues().unwrap();
    for (name, q) in &queues {
        b.save_queue(name, q).unwrap();
    }
    assert_eq!(downloads_json(&b), downloads_json(&a));
    assert_eq!(queues_json(&b), queues_json(&a));
}
