//! The per-download windows (phase E3): the progress window, Check hash, File Properties.

use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use lider_core::models::Status;

use crate::app::{App, Effect};
use crate::dialogs::checkhash::{self, CheckHash};
use crate::dialogs::progress::{self, Progress};
use crate::dialogs::properties::{self, Properties};
use crate::tests::{golden, harness, silent_server, start_running};
use crate::tests_dialogs::{click, shows};

type H = egui_kittest::Harness<'static, App>;

fn id_of(app: &App, name: &str) -> i64 {
    app.manager
        .downloads
        .values()
        .find(|d| d.filename == name)
        .unwrap_or_else(|| panic!("{name} is in the golden profile"))
        .id
}

/// A finished download of a real file in `dir`.
fn finished_file(app: &mut App, dir: &std::path::Path, name: &str, data: &[u8]) -> i64 {
    std::fs::write(dir.join(name), data).unwrap();
    let d = app.manager.add(
        "http://127.0.0.1:9/file",
        name,
        lider_core::manager::NewDownload {
            save_dir: Some(dir.to_string_lossy().into_owned()),
            ..Default::default()
        },
    );
    let n = data.len() as i64;
    app.manager.edit(d.id, |d| {
        d.status = Status::Complete;
        d.size = n;
        d.downloaded = n;
    });
    d.id
}

fn disabled(h: &H, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

/// Stop every download (the golden profile starts a queued one) and wait until none runs.
fn stop_everything(app: &mut App) {
    app.manager.settings.proxy.clear();
    app.manager.stop_all();
    let end = Instant::now() + Duration::from_secs(5);
    while app
        .manager
        .downloads
        .keys()
        .any(|i| app.manager.is_running(*i))
        && Instant::now() < end
    {
        std::thread::sleep(Duration::from_millis(20));
        app.manager.poll(None);
    }
}

/// Stopping ends the task a moment later.
fn wait_stopped(app: &mut App, id: i64) {
    let end = Instant::now() + Duration::from_secs(5);
    while app.manager.is_running(id) {
        assert!(Instant::now() < end, "the download stops");
        std::thread::sleep(Duration::from_millis(20));
    }
    app.manager.poll(None);
}

#[test]
fn check_hash_of_a_missing_file_cannot_run() {
    let (_tmp, mut app) = golden();
    let id = id_of(&app, "doc.pdf");
    app.selected = vec![id];
    let mut h = harness(app);
    h.state_mut().run("ctx_check_hash");
    h.run();
    assert!(shows(&h, "Check hash"));
    assert!(
        h.query_all_by_label_contains("File not found at")
            .next()
            .is_some(),
        "Check hash: the missing file is named"
    );
    assert!(
        disabled(&h, "Check now"),
        "Check hash: a missing file cannot be checked"
    );
    click(&mut h, "Close");
    assert!(h.state().dialogs.is_empty());
}

#[test]
fn check_hash_computes_and_verifies_a_file() {
    let (tmp, mut app) = golden();
    let id = finished_file(&mut app, tmp.path(), "abc.txt", b"abc");
    const SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let mut h = harness(app);
    checkhash::open(h.state_mut(), id);
    h.run();
    let label = format!("checkhash-{id}");
    let wait = |h: &mut H, want: checkhash::State| {
        let end = Instant::now() + Duration::from_secs(5);
        while h
            .state_mut()
            .dialog_body::<CheckHash>(&label)
            .unwrap()
            .state
            != want
        {
            assert!(Instant::now() < end, "the check reaches {want:?}");
            std::thread::sleep(Duration::from_millis(20));
            h.run();
        }
    };
    // No value: only compute.
    click(&mut h, "Compute hash");
    wait(&mut h, checkhash::State::Computed);
    let body = h.state_mut().dialog_body::<CheckHash>(&label).unwrap();
    assert_eq!(body.outcome.as_ref().unwrap().actual, SHA256);
    assert!(
        h.state().manager.downloads[&id].checksum_result.is_empty(),
        "nothing is saved"
    );
    // With the value: Matches, saved to the download.
    h.state_mut()
        .dialog_body::<CheckHash>(&label)
        .unwrap()
        .edit
        .text = SHA256.into();
    h.run();
    assert_eq!(
        h.state_mut()
            .dialog_body::<CheckHash>(&label)
            .unwrap()
            .state,
        checkhash::State::Ready,
        "editing the value goes back to ready"
    );
    click(&mut h, "Check now");
    wait(&mut h, checkhash::State::Match);
    assert!(shows(&h, "Matches — the file is intact"));
    let d = &h.state().manager.downloads[&id];
    assert_eq!(d.checksum_result, "verified");
    assert_eq!(d.checksum, format!("sha256:{SHA256}"));
    // A wrong value: mismatch; Keep file closes.
    h.state_mut()
        .dialog_body::<CheckHash>(&label)
        .unwrap()
        .edit
        .text = "0".repeat(64);
    h.run();
    click(&mut h, "Check now");
    wait(&mut h, checkhash::State::Mismatch);
    assert_eq!(h.state().manager.downloads[&id].checksum_result, "mismatch");
    click(&mut h, "Keep file");
    assert!(h.state().dialogs.is_empty());
}

#[test]
fn check_hash_of_a_running_download_waits_for_it() {
    let (tmp, app) = golden();
    let mut h = harness(app);
    let (_server, url) = silent_server();
    let id = start_running(h.state_mut(), &url, tmp.path());
    checkhash::open(h.state_mut(), id);
    h.run();
    assert!(disabled(&h, "Check when finished"), "a value is needed");
    h.state_mut()
        .dialog_body::<CheckHash>(&format!("checkhash-{id}"))
        .unwrap()
        .edit
        .text = "a".repeat(64);
    h.run();
    click(&mut h, "Check when finished");
    assert!(h.state().dialogs.is_empty());
    assert_eq!(
        h.state().manager.downloads[&id].checksum,
        format!("sha256:{}", "a".repeat(64))
    );
    h.state_mut().manager.shutdown();
}

#[test]
fn properties_edit_the_description() {
    let (_tmp, mut app) = golden();
    let id = id_of(&app, "doc.pdf");
    app.selected = vec![id];
    let mut h = harness(app);
    h.state_mut().run("ctx_properties");
    h.run();
    assert!(shows(&h, "File Properties"));
    for l in [
        "Type", "Size", "Save To", "Address", "Referer", "Login", "Password",
    ] {
        assert!(shows(&h, l), "File Properties has {l}");
    }
    let field = h.get_by_role_and_label(Role::TextInput, "Description");
    field.focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Description")
        .type_text(" changed in e2e");
    h.run();
    click(&mut h, "OK");
    assert!(h.state().dialogs.is_empty());
    assert!(
        h.state().manager.downloads[&id]
            .description
            .ends_with("changed in e2e"),
        "Properties OK writes the description"
    );
    // Double-click on a finished download opens Properties.
    let complete = h
        .state()
        .manager
        .downloads
        .values()
        .find(|d| d.status == Status::Complete)
        .unwrap()
        .id;
    h.state_mut().activate(complete);
    assert!(h.state().dialog_open(&properties::label(complete)));
}

#[test]
fn properties_move_a_finished_file_and_refuse_a_running_one() {
    let (tmp, mut app) = golden();
    let id = finished_file(&mut app, tmp.path(), "a.bin", b"data");
    let mut h = harness(app);
    properties::open(h.state_mut(), id);
    h.run();
    {
        let b = h
            .state_mut()
            .dialog_body::<Properties>(&properties::label(id))
            .unwrap();
        b.filename = "b.bin".into();
        b.save_dir = tmp.path().join("moved").to_string_lossy().into_owned();
    }
    click(&mut h, "OK");
    assert!(h.state().dialogs.is_empty());
    assert!(tmp.path().join("moved/b.bin").is_file(), "the file moved");
    assert!(!tmp.path().join("a.bin").exists());
    assert_eq!(h.state().manager.downloads[&id].filename, "b.bin");
    // A running download is not renamed.
    let (_server, url) = silent_server();
    let run = start_running(h.state_mut(), &url, tmp.path());
    properties::open(h.state_mut(), run);
    h.run();
    h.state_mut()
        .dialog_body::<Properties>(&properties::label(run))
        .unwrap()
        .filename = "other.bin".into();
    click(&mut h, "OK");
    assert!(shows(&h, "Stop the download before moving or renaming it."));
    assert_eq!(h.state().manager.downloads[&run].filename, "never.bin");
    h.state_mut().manager.shutdown();
}

#[test]
fn the_progress_window_follows_the_settings() {
    let (tmp, mut app) = golden();
    stop_everything(&mut app);
    let mut h = harness(app);
    let (_server, url) = silent_server();
    let id = h
        .state_mut()
        .manager
        .add(
            &url,
            "never.bin",
            lider_core::manager::NewDownload {
                save_dir: Some(tmp.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        )
        .id;
    // Double-click starts it and shows its window.
    h.state_mut().activate(id);
    h.run();
    assert!(h.state().dialog_open(&progress::label(id)));
    for l in [
        "Download status",
        "Speed Limiter",
        "Options on completion",
        "Transfer rate",
        "Resume capability",
        "Pause",
        "Cancel",
        "<< Hide details",
        lider_core::view::SEGMENTS,
    ] {
        assert!(shows(&h, l), "the progress window has {l}");
    }
    // The title follows the download.
    let title = &h
        .state()
        .dialogs
        .iter()
        .find(|d| d.label == progress::label(id))
        .unwrap()
        .title;
    assert!(title.ends_with("never.bin"), "title {title}");
    // Cancel pauses it and closes the window.
    click(&mut h, "Cancel");
    assert!(!h.state().dialog_open(&progress::label(id)));
    wait_stopped(h.state_mut(), id);
    // Resume of one download with the details pane hidden shows the window again.
    h.state_mut().manager.settings.show_details = false;
    h.state_mut().selected = vec![id];
    h.state_mut().checked.clear();
    h.state_mut().rebuild();
    h.state_mut().run("resume");
    assert!(h.state().dialog_open(&progress::label(id)));
    h.state_mut().close_dialog(&progress::label(id));
    // Options turn it off: no window, unless asked for (the details pane's button).
    h.state_mut().manager.stop(id);
    wait_stopped(h.state_mut(), id);
    h.state_mut().manager.settings.show_progress_dialog = false;
    h.state_mut().activate(id);
    assert!(!h.state().dialog_open(&progress::label(id)));
    progress::open(h.state_mut(), id, true);
    assert!(h.state().dialog_open(&progress::label(id)));
    // The speed limit tab applies and keeps the limit.
    h.run();
    {
        let b = h
            .state_mut()
            .dialog_body::<Progress>(&progress::label(id))
            .unwrap();
        b.tab = 1;
    }
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Use Speed Limiter")
        .click();
    h.run();
    let kbps = h
        .state_mut()
        .dialog_body::<Progress>(&progress::label(id))
        .unwrap()
        .limit_kbps;
    assert_eq!(
        h.state().manager.downloads[&id].speed_limit_kbps,
        kbps,
        "the limit is kept on the download"
    );
    h.state_mut().manager.shutdown();
}

#[test]
fn completion_options_of_the_progress_window() {
    use lider_core::manager::{Event, EventKind};
    let (_tmp, mut app) = golden();
    stop_everything(&mut app);
    let id = id_of(&app, "doc.pdf");
    let complete = |app: &mut App| {
        app.handle_events(&[Event {
            kind: EventKind::Complete,
            download_id: Some(id),
            queue: None,
            message: String::new(),
        }])
    };
    // "Show download complete dialog" off: a notification instead.
    progress::open(&mut app, id, true);
    app.dialog_body::<Progress>(&progress::label(id))
        .unwrap()
        .options
        .show_complete = false;
    complete(&mut app);
    assert!(
        !app.dialog_open(&progress::label(id)),
        "the progress window closes"
    );
    assert!(!app.dialog_open(&crate::dialogs::complete::label(id)));
    assert!(
        app.effects
            .iter()
            .any(|e| matches!(e, Effect::Notify(t, _) if t == "Download complete"))
    );
    // Without a progress window the setting decides: Download complete opens.
    complete(&mut app);
    assert!(app.dialog_open(&crate::dialogs::complete::label(id)));
    app.close_dialog(&crate::dialogs::complete::label(id));
    // "Exit LID when done".
    progress::open(&mut app, id, true);
    {
        let o = &mut app
            .dialog_body::<Progress>(&progress::label(id))
            .unwrap()
            .options;
        o.show_complete = false;
        o.exit = true;
    }
    complete(&mut app);
    assert!(app.quit, "LID exits");
}
