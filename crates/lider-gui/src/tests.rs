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

pub(crate) fn copy_dir(from: &Path, to: &Path) {
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
pub(crate) fn golden() -> (tempfile::TempDir, App) {
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

pub(crate) fn harness(app: App) -> Harness<'static, App> {
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
pub(crate) fn names(app: &App) -> Vec<String> {
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

// ---------------------------------------------------------------------- phase E2: running

use crate::app::{Effect, Request};

pub(crate) fn box_title(app: &App) -> String {
    app.boxx
        .as_ref()
        .map(|b| b.title.clone())
        .unwrap_or_default()
}

#[test]
fn other_starts_and_the_browser_reach_the_window() {
    let (_tmp, mut app) = golden();
    app.inbox.lock().unwrap().extend([
        "--show".to_string(),
        "https://example.com/a.zip".to_string(),
    ]);
    app.tick_now();
    assert!(
        app.effects.contains(&Effect::Show),
        "a second start shows the window"
    );
    assert!(
        app.boxx
            .as_ref()
            .is_some_and(|b| b.text.contains("https://example.com/a.zip")),
        "the address goes to the add flow"
    );
    app.boxx = None;
    app.effects.clear();
    app.on_line(
        r#"{"action":"download","url":"https://example.com/b.iso","page_url":"https://example.com/","cookies":"k=v","user_agent":"UA","filename":"b.iso"}"#,
    );
    match app.requests.last() {
        Some(Request::AddUrl {
            url,
            referer,
            headers,
            filename,
            ..
        }) => {
            assert_eq!(url, "https://example.com/b.iso");
            assert_eq!(referer, "https://example.com/", "the page is the referer");
            assert_eq!(headers["Cookie"], "k=v");
            assert_eq!(headers["User-Agent"], "UA");
            assert_eq!(filename, "b.iso");
        }
        r => panic!("a download request: {r:?}"),
    }
    app.on_line(
        r#"{"action":"links","page_url":"https://example.com/","items":[{"url":"https://example.com/1.zip","text":"one"},{"url":"javascript:void(0)"},{"url":"https://example.com/2.png","kind":"img"}]}"#,
    );
    match app.requests.last() {
        Some(Request::Links { items, .. }) => {
            assert_eq!(items.len(), 2, "only web addresses: {items:?}");
            assert_eq!(
                items[0],
                ("https://example.com/1.zip".into(), "one".into(), "a".into())
            );
            assert_eq!(items[1].2, "img");
        }
        r => panic!("a links request: {r:?}"),
    }
    assert!(app.effects.contains(&Effect::Show), "links show the window");
    app.on_line(r#"{"action":"show"}"#);
    app.on_line("{not json");
    assert_eq!(app.requests.len(), 2);
}

#[test]
fn the_clipboard_monitor_takes_new_addresses_of_known_types() {
    let (_tmp, mut app) = golden();
    app.clipboard_text("https://example.com/at-start.zip");
    assert!(
        app.requests.is_empty(),
        "what is there at start is not a change"
    );
    app.clipboard_text("https://example.com/file.zip");
    assert_eq!(app.requests, [Request::add("https://example.com/file.zip")]);
    app.clipboard_text("https://example.com/file.zip");
    app.clipboard_text("https://example.com/page.html");
    app.clipboard_text("some words");
    assert_eq!(
        app.requests.len(),
        1,
        "same text, other types and words are left alone"
    );
    app.manager.settings.monitor_clipboard = false;
    app.clipboard_text("https://example.com/off.zip");
    assert_eq!(app.requests.len(), 1, "the monitor can be turned off");
}

#[test]
fn ctrl_v_adds_the_pasted_addresses() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    h.event(egui::Event::Paste(
        "https://example.com/1.zip\nhttps://example.com/2.zip # note".into(),
    ));
    h.run();
    assert_eq!(h.state().view.rows.len(), 12, "two addresses are added");
    assert!(
        h.query_by_label(
            "2 file(s) were added to the \"Main download queue\". Start the queue now?"
        )
        .is_some()
            || box_title(h.state()) == "LID",
        "and the Main queue is offered"
    );
    h.get_by_label("No").click();
    h.run();
    h.event(egui::Event::Paste("https://example.com/one.iso".into()));
    h.run();
    assert_eq!(
        h.state().requests,
        [Request::add("https://example.com/one.iso")]
    );
}

/// A server that accepts and never answers: a download to it keeps running.
pub(crate) fn silent_server() -> (std::net::TcpListener, String) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/never.bin", l.local_addr().unwrap());
    (l, url)
}

pub(crate) fn start_running(app: &mut App, url: &str, dir: &Path) -> i64 {
    // The golden profile's proxy (127.0.0.1:8080) is not there: connect straight.
    app.manager.settings.proxy.clear();
    let d = app.manager.add(
        url,
        "never.bin",
        lider_core::manager::NewDownload {
            save_dir: Some(dir.to_string_lossy().into_owned()),
            start: true,
            ..Default::default()
        },
    );
    assert!(app.manager.is_running(d.id));
    d.id
}

#[test]
fn closing_follows_the_tray_and_the_running_downloads() {
    let (tmp, mut app) = golden();
    // The tray is up and closing goes there.
    app.tray_shown = true;
    assert!(!app.close_requested());
    assert_eq!(
        app.effects,
        [Effect::Hide],
        "the close button hides to the tray"
    );
    app.effects.clear();
    // Without the tray, a download keeps the window open with the question.
    app.tray_shown = false;
    let (_server, url) = silent_server();
    let id = start_running(&mut app, &url, tmp.path());
    assert!(!app.close_requested());
    let b = app.boxx.clone().unwrap();
    assert_eq!(b.title, "1 download is still running");
    assert!(b.text.starts_with("Closing the window stops it."));
    let ids: Vec<&str> = b.buttons.iter().map(|b| b.id).collect();
    assert_eq!(ids, ["stop", "background", "keep"]);
    assert_eq!(b.items[0].1, "never.bin");
    let mut h = harness(app);
    h.run();
    h.get_by_label("Keep downloading").click();
    h.run();
    assert!(
        h.state().manager.is_running(id),
        "Keep downloading keeps it"
    );
    assert!(!h.state().quit);
    // Exit with the tray up: no "background" (the tray is already there).
    h.state_mut().tray_shown = true;
    h.state_mut().run("exit");
    h.run();
    let ids: Vec<&str> = h
        .state()
        .boxx
        .as_ref()
        .unwrap()
        .buttons
        .iter()
        .map(|b| b.id)
        .collect();
    assert_eq!(ids, ["stop", "keep"]);
    assert!(h.query_by_label("Stop and exit").is_some());
    h.get_by_label("Stop and exit").click();
    h.run();
    let s = h.state();
    assert!(
        s.quit && s.effects.contains(&Effect::Close),
        "stop and exit closes"
    );
    let t = std::time::Instant::now();
    while s.manager.is_running(id) && t.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!s.manager.is_running(id), "the manager is stopped");
    assert!(
        h.state_mut().close_requested(),
        "the window's own close then goes through"
    );
}

#[test]
fn continue_in_background_turns_the_tray_on() {
    let (tmp, mut app) = golden();
    app.manager.settings.tray_icon = false;
    let (_server, url) = silent_server();
    start_running(&mut app, &url, tmp.path());
    assert!(!app.close_requested());
    let mut h = harness(app);
    h.run();
    h.get_by_label("Continue in background").click();
    h.run();
    let s = h.state();
    assert!(s.manager.settings.tray_icon && s.manager.settings.close_to_tray);
    assert!(s.effects.contains(&Effect::Hide));
    assert!(
        s.effects
            .iter()
            .any(|e| matches!(e, Effect::Notify(t, _) if t == "LID keeps downloading"))
    );
    assert!(!s.quit);
}

#[test]
fn nothing_running_closes_at_once() {
    let (_tmp, mut app) = golden();
    // The golden profile starts a queued download at launch: let it end first.
    app.manager.stop_all();
    let t = std::time::Instant::now();
    while app
        .manager
        .downloads
        .keys()
        .any(|i| app.manager.is_running(*i))
        && t.elapsed().as_secs() < 5
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    app.tray_shown = false;
    assert!(app.close_requested());
    assert!(app.effects.contains(&Effect::Close) && app.quit);
}

#[test]
fn the_power_off_countdown() {
    let (_tmp, mut app) = golden();
    app.start_power_off();
    assert_eq!(box_title(&app), "Shut down");
    assert!(app.boxx.as_ref().unwrap().text.contains("in 30 seconds"));
    let mut h = harness(app);
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(
        h.state().power_off_at.is_none(),
        "Cancel stops the countdown"
    );
    let app = h.state_mut();
    app.start_power_off();
    app.power_off_at = Some(std::time::Instant::now() + std::time::Duration::from_millis(2500));
    app.tick_now();
    assert!(
        app.boxx.as_ref().unwrap().text.contains("in 3 seconds"),
        "the box counts down"
    );
    app.power_off_at = Some(std::time::Instant::now());
    app.tick_now();
    assert!(
        app.effects.contains(&Effect::PowerOff),
        "at 0 the computer is turned off"
    );
    assert!(app.boxx.is_none());
}

#[test]
fn manager_events_become_notifications() {
    use lider_core::manager::{Event, EventKind};
    let (_tmp, mut app) = golden();
    let id = *app.manager.downloads.keys().next().unwrap();
    let name = app.manager.downloads[&id].filename.clone();
    let ev = |kind, message: &str| Event {
        kind,
        download_id: Some(id),
        queue: None,
        message: message.into(),
    };
    app.handle_events(&[
        ev(EventKind::Complete, ""),
        ev(EventKind::Error, "HTTP 404"),
        ev(EventKind::Mismatch, "kept"),
        ev(EventKind::Changed, "again"),
        ev(EventKind::Mismatch, ""),
    ]);
    let titles: Vec<&str> = app
        .effects
        .iter()
        .filter_map(|e| match e {
            Effect::Notify(t, _) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        titles,
        ["Download failed", "Checksum mismatch", "Checksum mismatch"]
    );
    assert!(app.effects.contains(&Effect::Notify(
        "Download failed".into(),
        format!("{name}\nHTTP 404")
    )));
    assert!(
        app.dialog_open(&crate::dialogs::complete::label(id)),
        "Download complete opens its window"
    );
    // With that window turned off, a notification says it.
    app.close_dialog(&crate::dialogs::complete::label(id));
    app.effects.clear();
    app.manager.settings.show_complete_dialog = false;
    app.handle_events(&[ev(EventKind::Complete, "")]);
    assert_eq!(
        app.effects,
        [Effect::Notify("Download complete".into(), name.clone())]
    );
    app.effects.clear();
    assert_eq!(
        app.requests,
        [Request::Mismatch(id)],
        "a mismatch to decide opens its window"
    );
}

#[test]
fn follow_the_system_theme() {
    let (_tmp, mut app) = golden();
    app.manager.settings.follow_system = true;
    app.manager.settings.dark_theme = "dark".into();
    app.dark_system = true;
    app.apply_theme();
    assert_eq!(app.theme, ThemeName::Dark);
    app.dark_system = false;
    app.apply_theme();
    assert_eq!(app.theme, ThemeName::Ember, "a light desktop gets Ember");
}

#[test]
fn the_menu_is_offered_once() {
    let (_tmp, mut app) = golden();
    app.manager.settings.menu_offered = false;
    app.offer_menu("LID-2.0.0-x86_64.AppImage");
    assert!(
        app.manager.settings.menu_offered,
        "remembered before the answer"
    );
    let saved = lider_core::settings::load(&app.dirs);
    assert!(saved.menu_offered, "and saved");
    let mut h = harness(app);
    h.run();
    assert!(
        h.query_by_label("Add LID to the application menu?")
            .is_some()
    );
    h.get_by_label("Not now").click();
    h.run();
    assert!(h.state().boxx.is_none());
}
