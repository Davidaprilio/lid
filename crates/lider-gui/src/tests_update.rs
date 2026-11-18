//! Phase E4 in the harness: the offer of a newer LID, the "Updating LID" window (download, signature,
//! install) and "Download yt-dlp?", against local servers (nothing reaches GitHub).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};

use base64::Engine;
use egui_kittest::kittest::Queryable;
use lider_core::update::Found;

use crate::app::{App, Request};
use crate::boxes::Then;
use crate::tests::{golden, harness};
use crate::tests_dialogs::{click, shows};

/// Serve these paths on a local port (anything else: 404); the paths may name the server's own address
/// as `{base}`.
fn serve(files: HashMap<&'static str, Vec<u8>>) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let files: HashMap<&str, Vec<u8>> = files
        .into_iter()
        .map(|(k, v)| {
            let v = String::from_utf8(v.clone())
                .map(|t| t.replace("{base}", &base).into_bytes())
                .unwrap_or(v);
            (k, v)
        })
        .collect();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            let _ = r.read_line(&mut line);
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                    break;
                }
            }
            let mut s = s;
            match files.get(path.as_str()) {
                Some(b) => {
                    let _ = write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        b.len()
                    );
                    let _ = s.write_all(b);
                }
                None => {
                    let _ = s.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
            }
        }
    });
    base
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// The golden app without the profile's proxy (127.0.0.1:8080 is not there).
fn app() -> (tempfile::TempDir, App) {
    let (tmp, mut app) = golden();
    app.manager.settings.proxy.clear();
    (tmp, app)
}

/// Tick until `done` holds (background work reports through the tick).
fn wait(h: &mut egui_kittest::Harness<'static, App>, done: impl Fn(&App) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done(h.state()) {
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(30));
        h.state_mut().tick_now();
        h.run();
    }
}

fn found(can_install: bool) -> Found {
    Found {
        version: "9.0.0".into(),
        notes: "Faster downloads.".into(),
        page: "https://example.invalid/v9".into(),
        latest_json: Some("http://127.0.0.1:9/latest.json".into()),
        can_install,
    }
}

#[test]
fn check_now_offers_the_newer_version_and_skip_remembers_it() {
    let list = serde_json::json!([
        {"tag_name": "v99.0.0", "prerelease": false, "draft": false, "html_url": "https://x/v99",
         "body": "Big release", "assets": []},
    ]);
    let base = serve(HashMap::from([(
        "/releases",
        list.to_string().into_bytes(),
    )]));
    let (_tmp, app) = app();
    let mut h = harness(app);
    h.state_mut()
        .check_for_update_at(&format!("{base}/releases"), false);
    wait(&mut h, |a| a.update_check.is_none());
    assert_eq!(h.state().update_note, "LID 99.0.0 is available.");
    assert!(shows(&h, "LID 99.0.0 is available"));
    // Not an AppImage: the package manager updates it, so the page is offered.
    assert!(shows(&h, "Open release page"));
    click(&mut h, "Skip this version");
    assert_eq!(lider_core::update::load(&h.state().dirs).skipped, "99.0.0");
    // At start, a skipped version is not offered again.
    h.state_mut()
        .check_for_update_at(&format!("{base}/releases"), true);
    wait(&mut h, |a| a.update_check.is_none());
    assert!(h.state().boxx.is_none(), "the skipped version stays quiet");
    // Nothing newer.
    let base = serve(HashMap::from([("/releases", b"[]".to_vec())]));
    h.state_mut()
        .check_for_update_at(&format!("{base}/releases"), false);
    wait(&mut h, |a| a.update_check.is_none());
    assert_eq!(h.state().update_note, "LID is up to date.");
}

#[test]
fn the_offer_of_an_installable_update() {
    let (_tmp, app) = app();
    let mut h = harness(app);
    h.state_mut().offer_update(found(true));
    h.run();
    for b in ["Skip this version", "Later", "Update now"] {
        assert!(shows(&h, b), "the offer has {b}");
    }
    assert!(shows(
        &h,
        "LID downloads it, checks its signature, installs it and starts again."
    ));
    click(&mut h, "Later");
    assert!(h.state().boxx.is_none());
    assert!(matches!(
        Then::Update(Box::new(found(false))),
        Then::Update(_)
    ));
}

#[test]
fn updating_checks_the_signature_before_anything_is_replaced() {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let pk = b64(&kp.pk.to_box().unwrap().to_string());
    let data = b"\x7fELF a newer LID".repeat(500);
    let sign = |d: &[u8]| {
        b64(
            &minisign::sign(None, &kp.sk, std::io::Cursor::new(d), None, None)
                .unwrap()
                .to_string(),
        )
    };
    let manifest = |sig: &str| {
        serde_json::json!({"version": "9.0.0", "platforms": {
            lider_core::update::platform_key(): {"url": "{base}/LID.AppImage", "signature": sig}}})
        .to_string()
        .into_bytes()
    };
    let (_tmp, app) = app();
    let mut h = harness(app);

    // Signed by another key: refused, nothing installed.
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let bad = b64(
        &minisign::sign(None, &other.sk, std::io::Cursor::new(&data[..]), None, None)
            .unwrap()
            .to_string(),
    );
    let base = serve(HashMap::from([
        ("/latest.json", manifest(&bad)),
        ("/LID.AppImage", data.clone()),
    ]));
    crate::dialogs::update::install_with_key(
        h.state_mut(),
        "9.0.0",
        &format!("{base}/latest.json"),
        &pk,
    );
    // (The download may end before the first frame, so the window is checked before drawing.)
    assert!(h.state().dialog_open("update"), "Updating LID opens");
    wait(&mut h, |a| a.dialogs.is_empty());
    assert!(shows(&h, "LID was not updated"));
    assert!(shows(&h, "the update is not signed with LID's key"));
    assert!(!h.state().quit, "LID keeps running");
    h.state_mut().boxx = None;

    // Signed with the key, but this copy is no AppImage (the tests do not run from one).
    let base = serve(HashMap::from([
        ("/latest.json", manifest(&sign(&data))),
        ("/LID.AppImage", data.clone()),
    ]));
    crate::dialogs::update::install_with_key(
        h.state_mut(),
        "9.0.0",
        &format!("{base}/latest.json"),
        &pk,
    );
    wait(&mut h, |a| a.dialogs.is_empty());
    if std::env::var_os("APPIMAGE").is_none() && !cfg!(windows) {
        assert!(shows(&h, "This copy of LID is not an AppImage."));
        assert!(h.state().restart.lock().unwrap().is_none());
    }
}

#[test]
fn download_yt_dlp_is_offered_and_cancel_closes_it() {
    let (_tmp, app) = app();
    let mut h = harness(app);
    crate::dialogs::update::offer_yt_dlp(
        h.state_mut(),
        Request::add("https://www.youtube.com/watch?v=x"),
    );
    h.run();
    if lider_core::tools::yt_dlp_asset().is_none() {
        assert!(
            h.state().boxx.is_some(),
            "no program for this system: a note"
        );
        return;
    }
    assert!(shows(&h, "Download yt-dlp?"));
    assert!(
        h.query_by_label_contains("checked against the release's checksums")
            .is_some()
    );
    click(&mut h, "Cancel");
    assert!(h.state().dialogs.is_empty());
}
