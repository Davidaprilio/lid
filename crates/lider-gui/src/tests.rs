//! The main window in a test harness (egui_kittest), on a copy of the 1.2 golden profile: the checks of the
//! Tauri window's UI scenario (ui/src/dev/e2e.ts), which came from test_ui.py in 1.x.

use std::path::Path;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lider_core::manager::DownloadManager;
use lider_core::paths::Dirs;
use lider_core::store::Store;
use lider_core::view::CheckState;

use crate::app::App;
use crate::main_window;
use crate::theme::ThemeName;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// The app on a temporary copy of tests/golden/v1.2 (the real home is never touched).
fn golden() -> (tempfile::TempDir, App) {
    let tmp = tempfile::tempdir().unwrap();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/v1.2");
    copy_dir(&golden, tmp.path());
    let dirs = Dirs::from_parts(
        tmp.path().into(),
        Some(tmp.path().join("config").into()),
        Some(tmp.path().join("data").into()),
        Some(tmp.path().join("cache").into()),
    );
    let settings = lider_core::settings::load(&dirs);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let store = Store::open(&dirs.database_file()).unwrap();
    let manager = DownloadManager::new(settings, store, rt.handle().clone()).unwrap();
    let mut app = App::new(rt, manager, dirs);
    app.tick();
    (tmp, app)
}

fn harness(app: App) -> Harness<'static, App> {
    // The same system fonts as the program (decision G1): CJK file names must have their glyphs.
    let mut fonts_ready = false;
    Harness::builder()
        .with_size(egui::vec2(1200.0, 760.0))
        .build_ui_state(
            move |ui, app: &mut App| {
                if !fonts_ready {
                    crate::fonts::install(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    fonts_ready = true;
                }
                main_window::show(app, ui);
            },
            app,
        )
}

/// The names of the rows as the list shows them, in order.
fn names(app: &App) -> Vec<String> {
    let at = app
        .view
        .columns
        .iter()
        .position(|c| c.key == "filename")
        .unwrap();
    let offset = app.view.columns.len() - app.view.rows.first().map_or(0, |r| r.cells.len());
    app.view
        .rows
        .iter()
        .map(|r| r.cells[at - offset].clone())
        .collect()
}

#[test]
fn the_golden_profile_and_the_layout() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    assert_eq!(
        h.state().view.rows.len(),
        10,
        "the golden profile shows 10 downloads"
    );
    for m in ["Tasks", "File", "Downloads", "View", "Help"] {
        assert!(h.query_by_label(m).is_some(), "menu bar has {m}");
    }
    for n in [
        "All Downloads",
        "Unfinished",
        "Finished",
        "Grabber projects",
        "Queues",
    ] {
        assert!(h.query_by_label(n).is_some(), "category tree has {n}");
    }
    for c in [
        "Compressed",
        "Documents",
        "Music",
        "Programs",
        "Video",
        "Images",
    ] {
        assert!(h.query_by_label(c).is_some(), "All Downloads lists {c}");
    }
    assert!(
        h.query_by_label("General").is_none(),
        "General is not listed"
    );
}

#[test]
fn tree_filters_the_list() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    h.get_by_label("Finished").click();
    h.run();
    let mut finished = names(h.state());
    finished.sort();
    assert_eq!(
        finished,
        ["bad.zip", "doc.pdf", "state-verifying.bin"],
        "Finished shows the complete downloads"
    );
    h.get_by_label("All Downloads").click();
    h.run();
    assert_eq!(h.state().view.rows.len(), 10);
}

#[test]
fn header_clicks_sort() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    h.get_by_label("Column File Name").click();
    h.run();
    assert_eq!(names(h.state())[0], "bad.zip", "File Name sorts ascending");
    h.get_by_label("Column File Name").click();
    h.run();
    assert_eq!(
        names(h.state())[0],
        "Video title.mp4",
        "a second click sorts descending"
    );
}

#[test]
fn checks_drive_the_header_box() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    let ids: Vec<i64> = h.state().view.rows.iter().take(2).map(|r| r.id).collect();
    for id in &ids {
        h.get_by_label(&format!("Check {id}")).click();
        h.run();
    }
    let s = h.state();
    assert_eq!(
        lider_core::view::check_state(&s.visible, &s.checked),
        CheckState::Partial,
        "two checks make the header box partial"
    );
    h.get_by_label("Check all").click();
    h.run();
    assert_eq!(
        h.state().checked.len(),
        10,
        "the header box checks every row"
    );
    assert!(
        h.query_by_label("10 checked").is_some(),
        "the toolbar shows the count"
    );
    h.get_by_label("Check all").click();
    h.run();
    assert!(
        h.state().checked.is_empty(),
        "a second click unchecks every row"
    );
}

#[test]
fn details_pane_follows_the_selection_and_f9() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    assert!(
        !h.state().manager.settings.show_details,
        "the pane is off in the golden profile"
    );
    h.key_press(Key::F9);
    h.run();
    assert!(
        h.query_by_label(
            "Select a download to see its progress, connections and speed limit here."
        )
        .is_some()
    );
    h.get_by_label("Download big.tar.gz").click();
    h.run();
    assert_eq!(
        h.state().view.details.as_ref().unwrap().percent,
        "30.0%",
        "the details pane shows the selected download"
    );
    assert!(h.query_by_label("30.0%").is_some());
    h.key_press(Key::F9);
    h.run();
    assert!(
        !h.state().manager.settings.show_details,
        "F9 hides the details pane"
    );
}

#[test]
fn search_filters_and_find_next() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    h.key_press_modifiers(Modifiers::CTRL, Key::F);
    h.run();
    assert!(h.state().search_open, "Ctrl+F opens the search bar");
    h.state_mut().search.flags |= 1; // Match case
    h.state_mut().search_text = "STATE".into();
    h.get_by_label("Find").click();
    h.run();
    assert!(
        h.state().view.rows.is_empty(),
        "with Match case on, \"STATE\" matches nothing"
    );
    h.state_mut().search.flags &= !1;
    h.state_mut().search_text = "state".into();
    h.get_by_label("Find").click();
    h.run();
    let found = names(h.state());
    assert!(
        !found.is_empty() && found.iter().all(|n| n.starts_with("state-")),
        "search filters the list: {found:?}"
    );
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.state().search_open);
    assert_eq!(
        h.state().view.rows.len(),
        10,
        "Esc closes the search bar and shows everything"
    );
    h.key_press(Key::F3);
    h.run();
    let s = h.state();
    let first = s
        .selected
        .first()
        .and_then(|id| s.manager.downloads.get(id))
        .map(|d| d.filename.clone())
        .unwrap_or_default();
    assert!(
        first.starts_with("state-"),
        "F3 selects the next match with the bar closed: {first}"
    );
}

#[test]
fn delete_asks_then_removes() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    h.get_by_label("Download stream.bin").click();
    h.run();
    h.key_press(Key::Delete);
    h.run();
    assert!(
        h.query_by_label("Confirm deletion of downloads").is_some(),
        "Delete asks first"
    );
    h.get_by_label("Yes").click();
    h.run();
    assert!(
        !names(h.state()).contains(&"stream.bin".to_string()),
        "Yes removes the download"
    );
    assert_eq!(h.state().view.rows.len(), 9);
}

#[test]
fn view_menu_theme_toolbar_and_tips() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    // The View menu ends with Appearance... (the menu description both windows draw).
    let view = h
        .state()
        .view
        .menus
        .iter()
        .find(|m| main_window::plain(&m.label) == "View")
        .cloned()
        .unwrap();
    let last = view.items.iter().rev().find_map(|i| match i {
        lider_core::menu::Item::Action { label, .. } => Some(main_window::plain(label)),
        _ => None,
    });
    assert_eq!(
        last.as_deref(),
        Some("Appearance..."),
        "the View menu ends with Appearance..."
    );
    h.state_mut().run("theme:ember");
    h.run();
    assert_eq!(
        h.state().theme,
        ThemeName::Ember,
        "View → Theme switches the palette"
    );
    h.state_mut().run("theme:dark");
    h.run();
    assert_eq!(h.state().theme, ThemeName::Dark);
    h.state_mut().run("toolbar_labels");
    h.run();
    for c in [
        "Add URL",
        "Start",
        "Stop",
        "Delete",
        "Options",
        "Scheduler",
        "Grabber",
    ] {
        assert!(h.query_by_label(c).is_some(), "toolbar caption {c}");
    }
    h.get_by_label("Options").hover();
    h.run();
    assert_eq!(
        h.state().status_tip,
        "Integration, file types, save folders, connections, proxy and others",
        "the tip of the button under the mouse shows in the status bar"
    );
}
