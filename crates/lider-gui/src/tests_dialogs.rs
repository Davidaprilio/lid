//! The dialog windows in the test harness, where each one is drawn as an egui window over the main
//! window: the small settings dialogs, Download complete, Checksum mismatch (the checks of the Tauri
//! build's dialog scenario, ui/src/dev/e2e-dialogs.ts, and of 1.x).

use egui::Key;
use egui_kittest::kittest::Queryable;

use crate::dialogs::complete::{Complete, Mismatch};
use crate::dialogs::small::{Columns, Find, Limiter};
use crate::tests::{golden, harness};

type H = egui_kittest::Harness<'static, crate::app::App>;

/// Press the button with this label.
pub(crate) fn click(h: &mut H, label: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Button, label)
        .click();
    h.run();
}

/// Something with this label shows (the window title and the header may both say it).
pub(crate) fn shows(h: &H, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

#[test]
fn a_dialog_opens_once_and_closes_with_its_buttons() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    h.state_mut().run("limiter_settings");
    h.state_mut().run("limiter_settings");
    h.run();
    assert_eq!(h.state().dialogs.len(), 1, "the same window comes forward");
    assert!(shows(&h, "Speed Limiter settings"));
    click(&mut h, "Cancel");
    assert!(h.state().dialogs.is_empty(), "Cancel closes it");
    // Escape cancels too.
    h.state_mut().run("limiter_settings");
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(h.state().dialogs.is_empty(), "Escape closes it");
}

#[test]
fn the_limiter_writes_its_speed() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("limiter_settings");
    h.run();
    let body = h.state_mut().dialog_body::<Limiter>("limiter").unwrap();
    assert_eq!(body.kbps, 512, "the limiter shows the saved speed");
    body.kbps = 300;
    // Enter presses OK.
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().dialogs.is_empty());
    assert_eq!(h.state().manager.settings.speed_limit_kbps, 300);
}

#[test]
fn columns_hide_one_and_reset() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("columns");
    h.run();
    for title in ["Q", "File Name", "Transfer rate", "Parent Web Page"] {
        assert!(shows(&h, title), "Columns lists {title}");
    }
    let body = h.state_mut().dialog_body::<Columns>("columns").unwrap();
    assert_eq!(
        body.cols.len(),
        12,
        "the Columns dialog lists the 12 columns"
    );
    let q = body.cols.iter().position(|c| c.0 == "queue").unwrap();
    body.cols[q].1 = false;
    // "File Name" always shows.
    let f = body.cols.iter().position(|c| c.0 == "filename").unwrap();
    body.cols[f].1 = false;
    click(&mut h, "OK");
    let keys: Vec<String> = h
        .state()
        .view
        .columns
        .iter()
        .map(|c| c.key.clone())
        .collect();
    assert!(!keys.contains(&"queue".into()), "the Q column is hidden");
    assert!(keys.contains(&"filename".into()), "File Name stays");
    h.state_mut().run("columns");
    h.run();
    click(&mut h, "Reset");
    let keys: Vec<String> = h
        .state()
        .view
        .columns
        .iter()
        .map(|c| c.key.clone())
        .collect();
    assert!(
        keys.contains(&"queue".into()),
        "Reset puts back the defaults"
    );
}

#[test]
fn search_settings_and_find_file() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("find_settings");
    h.run();
    assert!(shows(&h, "LID Search settings"));
    let body = h.state_mut().dialog_body::<Find>("find").unwrap();
    body.flags |= lider_core::search::flags::MATCH_CASE;
    click(&mut h, "OK");
    assert_eq!(
        h.state().manager.settings.search_flags & 1,
        1,
        "OK writes the flags"
    );
    // No target: the box says so and the name target comes back.
    h.state_mut().run("find_settings");
    h.run();
    h.state_mut().dialog_body::<Find>("find").unwrap().flags = 0;
    click(&mut h, "OK");
    assert!(shows(&h, lider_core::search::NO_TARGET));
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state_mut().dialog_body::<Find>("find").unwrap().flags, 4);
    click(&mut h, "Cancel");
    // F3 without a query opens "Find file"; Find selects the match.
    h.state_mut().search.query.clear();
    h.state_mut().run("find_next");
    h.run();
    assert!(shows(&h, "Find file"));
    h.state_mut().dialog_body::<Find>("find").unwrap().query = "doc".into();
    click(&mut h, "Find");
    assert!(h.state().dialogs.is_empty());
    let id = h.state().selected[0];
    assert!(h.state().manager.downloads[&id].filename.contains("doc"));
    // Nothing matches: the window stays with the note.
    h.state_mut().search.query.clear();
    h.state_mut().run("find_next");
    h.run();
    h.state_mut().dialog_body::<Find>("find").unwrap().query = "no-such-file-anywhere".into();
    click(&mut h, "Find");
    assert!(shows(&h, "No such files found"));
}

#[test]
fn a_new_category_and_its_properties() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("cat_add");
    h.run();
    assert!(shows(&h, "New category"));
    {
        let body = h
            .state_mut()
            .dialog_body::<crate::dialogs::small::CategoryWindow>("category")
            .unwrap();
        body.form.name = "E2E files".into();
        body.form.types.text = "abc, .DEF".into();
    }
    click(&mut h, "OK");
    let cat = h
        .state()
        .manager
        .settings
        .categories
        .iter()
        .find(|c| c.name == "E2E files")
        .cloned()
        .expect("the category is added");
    assert_eq!(
        cat.extensions,
        ["abc", "def"],
        "types without dots, lower case"
    );
    assert!(
        h.state().view.tree.iter().any(
            |n| n.title == "All Downloads" && n.children.iter().any(|c| c.title == "E2E files")
        ),
        "a new category shows in the sidebar"
    );
    // Renaming moves its downloads.
    let id = *h.state().manager.downloads.keys().next().unwrap();
    h.state_mut()
        .manager
        .edit(id, |d| d.category = "E2E files".into());
    h.state_mut().run("cat_properties:E2E files");
    h.run();
    assert!(shows(&h, "Edit the E2E files category"));
    h.state_mut()
        .dialog_body::<crate::dialogs::small::CategoryWindow>("category")
        .unwrap()
        .form
        .name = "Renamed".into();
    click(&mut h, "OK");
    assert_eq!(h.state().manager.downloads[&id].category, "Renamed");
}

#[test]
fn download_complete_and_dont_show_again() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    let id = h
        .state()
        .manager
        .downloads
        .values()
        .find(|d| d.status == lider_core::models::Status::Complete)
        .unwrap()
        .id;
    crate::dialogs::complete::complete(h.state_mut(), id);
    h.run();
    assert!(shows(&h, "Download complete"));
    for b in ["Open", "Open with...", "Open folder", "Close"] {
        assert!(shows(&h, b), "Download complete has {b}");
    }
    h.state_mut()
        .dialog_body::<Complete>(&crate::dialogs::complete::label(id))
        .unwrap()
        .dont_show = true;
    click(&mut h, "Close");
    assert!(
        !h.state().manager.settings.show_complete_dialog,
        "Don't show this dialog again applies when the window closes"
    );
}

#[test]
fn a_mismatch_keeps_or_deletes_the_file() {
    let (tmp, app) = golden();
    let mut h = harness(app);
    let file = tmp.path().join("bad.bin");
    std::fs::write(&file, b"not what was expected").unwrap();
    let id = {
        let m = &mut h.state_mut().manager;
        let d = m.add(
            "http://127.0.0.1:9/bad.bin",
            "bad.bin",
            lider_core::manager::NewDownload {
                save_dir: Some(tmp.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        m.edit(d.id, |d| {
            d.status = lider_core::models::Status::Complete;
            d.checksum = format!("sha256:{}", "0".repeat(64));
            d.checksum_actual = "f".repeat(64);
            d.checksum_result = "mismatch".into();
        });
        d.id
    };
    h.state_mut()
        .requests
        .push(crate::app::Request::Mismatch(id));
    h.state_mut().take_requests();
    h.run();
    assert!(shows(&h, "The file doesn't match its checksum"));
    assert!(
        h.state_mut()
            .dialog_body::<Mismatch>(&format!("mismatch-{id}"))
            .is_some()
    );
    click(&mut h, "Keep file");
    assert!(
        h.state().manager.downloads.contains_key(&id),
        "Keep file keeps it"
    );
    assert!(file.exists());
    crate::dialogs::complete::mismatch(h.state_mut(), id);
    h.run();
    // Deleting moves the file to the Trash, which is in the real home: the file goes first here, so only
    // the entry is left to remove.
    std::fs::remove_file(&file).unwrap();
    click(&mut h, "Delete file");
    assert!(h.state().dialogs.is_empty());
    assert!(
        !h.state().manager.downloads.contains_key(&id),
        "Delete file removes it"
    );
}
