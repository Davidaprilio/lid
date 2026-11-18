//! The settings windows in the test harness: LID options, Scheduler, Browser integration (the checks of
//! the Tauri build's dialog scenario, ui/src/dev/e2e-dialogs.ts). Desktop entries (autostart, the
//! application menu) and browser hosts are never written here: they live in the real home.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use lider_core::models::{MAIN_QUEUE, SYNC_QUEUE};

use crate::dialogs::options::{Options, TABS};
use crate::dialogs::scheduler::Scheduler;
use crate::tests::{golden, harness};
use crate::tests_dialogs::{click, click_top, shows};

type H = egui_kittest::Harness<'static, crate::app::App>;

/// Click a tab of the open dialog (drawn after the main window, whose menu may have the same word).
fn tab(h: &mut H, name: &str) {
    h.query_all_by_label(name).last().unwrap().click();
    h.run();
}

#[test]
fn options_has_seven_tabs_and_ok_writes_the_appearance() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("appearance");
    h.run();
    assert_eq!(
        TABS,
        [
            "General",
            "File types",
            "Save to",
            "Downloads",
            "Connection",
            "Proxy / Socks",
            "Appearance"
        ],
        "Options has 7 tabs in order"
    );
    for t in TABS {
        assert!(shows(&h, t), "the tab strip shows {t}");
    }
    assert_eq!(
        h.state_mut().dialog_body::<Options>("options").unwrap().tab,
        6,
        "View → Appearance... opens the Appearance tab"
    );
    click(&mut h, "Compact");
    click(&mut h, "Icons only");
    // Every tab draws (and names its fields).
    for t in TABS {
        tab(&mut h, t);
    }
    click(&mut h, "OK");
    assert!(h.state().dialogs.is_empty(), "OK closes Options");
    let s = &h.state().manager.settings;
    assert_eq!(s.density, "compact", "Options OK writes the density");
    assert!(!s.toolbar_labels);
}

#[test]
fn options_general_and_cancel() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("options");
    h.run();
    assert!(shows(&h, "Desktop Integration"));
    let o = h.state_mut().dialog_body::<Options>("options").unwrap();
    assert_eq!(o.tab, 0);
    o.s.monitor_clipboard = false;
    o.launch = !o.desk.autostart;
    click(&mut h, "Cancel");
    assert!(h.state().dialogs.is_empty());
    assert!(
        h.state().manager.settings.monitor_clipboard,
        "Cancel writes nothing"
    );
    // OK writes launch_on_startup (the autostart entry itself is left alone under test).
    h.state_mut().run("options");
    h.run();
    let o = h.state_mut().dialog_body::<Options>("options").unwrap();
    o.launch = true;
    o.s.monitor_clipboard = false;
    click(&mut h, "OK");
    let s = &h.state().manager.settings;
    assert!(s.launch_on_startup);
    assert!(!s.monitor_clipboard);
}

#[test]
fn a_category_and_a_server_limit_from_options() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("options");
    h.run();
    tab(&mut h, "Save to");
    click(&mut h, "New");
    assert!(
        shows(&h, "New category"),
        "the category form opens over Options"
    );
    {
        let o = h.state_mut().dialog_body::<Options>("options").unwrap();
        let (form, _) = o.edit_cat.as_mut().unwrap();
        form.name = "E2E files".into();
        form.types.text = "e2e".into();
    }
    click_top(&mut h, "OK");
    let o = h.state_mut().dialog_body::<Options>("options").unwrap();
    assert!(o.edit_cat.is_none(), "OK goes back to the tabs");
    assert_eq!(
        o.cats[o.cat_index].name, "E2E files",
        "the new category is selected"
    );
    // A server limit.
    tab(&mut h, "Connection");
    click(&mut h, "New");
    assert!(shows(&h, "Max. connections number for a server"));
    h.get_by_role_and_label(Role::TextInput, "Server").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Server")
        .type_text("*.e2e.test");
    h.run();
    click_top(&mut h, "OK");
    assert!(shows(&h, "*.e2e.test"), "the exception is listed");
    click(&mut h, "OK");
    let s = &h.state().manager.settings;
    let cat = s.categories.iter().find(|c| c.name == "E2E files").unwrap();
    assert_eq!(cat.extensions, ["e2e"]);
    assert_eq!(s.server_limits.get("*.e2e.test"), Some(&2));
    assert!(
        h.state()
            .view
            .tree
            .iter()
            .any(|n| n.children.iter().any(|c| c.title == "E2E files")),
        "a new category shows in the sidebar"
    );
}

#[test]
fn scheduler_lists_queues_and_adds_one() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("scheduler");
    h.run();
    assert_eq!(
        lider_core::view::queue_names(&h.state().manager)[..2],
        [MAIN_QUEUE.to_string(), SYNC_QUEUE.to_string()],
        "Scheduler lists Main and Synchronization first"
    );
    assert_eq!(
        h.state_mut()
            .dialog_body::<Scheduler>("scheduler")
            .unwrap()
            .queue,
        MAIN_QUEUE
    );
    click(&mut h, "New queue");
    let text = "Enter a name for the new queue that will be displayed in the list of queues";
    h.get_by_role_and_label(Role::TextInput, text)
        .type_text("E2E queue");
    h.run();
    click(&mut h, "OK");
    assert_eq!(
        h.state_mut()
            .dialog_body::<Scheduler>("scheduler")
            .unwrap()
            .queue,
        "E2E queue",
        "New queue adds and selects it"
    );
    assert!(
        h.state().view.queues.contains(&"E2E queue".to_string()),
        "the new queue shows in the sidebar"
    );
    // The default queues cannot be deleted.
    h.state_mut()
        .dialog_body::<Scheduler>("scheduler")
        .unwrap()
        .tab = 0;
    click(&mut h, "Close");
    assert!(h.state().dialogs.is_empty(), "Close closes the Scheduler");
    h.state_mut().run("scheduler");
    h.run();
    click(&mut h, "Delete");
    assert!(shows(&h, "The default queues cannot be deleted."));
    click(&mut h, "OK");
    assert!(h.state().manager.queues.contains_key(MAIN_QUEUE));
    click(&mut h, "Close");
}

#[test]
fn scheduler_keeps_the_schedule_and_deletes_a_queue() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.state_mut().run("q_scheduler:Nightly");
    h.run();
    {
        let s = h.state_mut().dialog_body::<Scheduler>("scheduler").unwrap();
        assert_eq!(s.queue, "Nightly");
        s.sch.exit_when_done = true;
        s.sch.start_time = "25:99".into();
    }
    click(&mut h, "Apply");
    let q = &h.state().manager.queues["Nightly"].schedule;
    assert!(q.exit_when_done, "Apply writes the schedule");
    assert!(
        lider_core::queue::parse_hhmm(&q.start_time).is_some(),
        "a bad time is not stored"
    );
    tab(&mut h, "Files in the queue");
    click(&mut h, "Delete");
    assert!(shows(&h, "Delete queue \"Nightly\"?"));
    click(&mut h, "Yes");
    assert!(!h.state().manager.queues.contains_key("Nightly"));
    assert_eq!(
        h.state_mut()
            .dialog_body::<Scheduler>("scheduler")
            .unwrap()
            .queue,
        MAIN_QUEUE
    );
}

#[test]
fn browser_integration_lists_the_browsers() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    h.run();
    assert!(
        shows(&h, "Browser: connected") || shows(&h, "Browser: not set up"),
        "the status bar shows the Browser link"
    );
    h.state_mut().run("browsers");
    h.run();
    assert!(shows(&h, "Browser integration"));
    use lider_sys::platform::prelude::*;
    for b in os().browsers() {
        assert!(shows(&h, b.name), "the window lists {}", b.name);
    }
    assert!(shows(&h, "Install for checked browsers"));
    click(&mut h, "Close");
    assert!(h.state().dialogs.is_empty());
}
