//! Adding downloads in the test harness: Batch download, the add flow with Download File Info, the
//! duplicate rule, Download All Links and the grabber (the "add" and "batch" checks of the Tauri build's
//! dialog scenario, ui/src/dev/e2e-dialogs.ts). Addresses are local only.

use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use lider_core::models::{MAIN_QUEUE, Status};

use crate::app::{App, Request};
use crate::dialogs::add::{Add, Batch, Page};
use crate::dialogs::links::Links;
use crate::tests::{golden, harness};
use crate::tests_dialogs::{click, shows};

type H = egui_kittest::Harness<'static, App>;

/// A closed local port: a connection is refused at once.
const CLOSED: &str = "http://127.0.0.1:9";

fn body<T: 'static>(h: &mut H) -> &mut T {
    h.state_mut()
        .dialogs
        .iter_mut()
        .find_map(|d| d.body.as_any().downcast_mut::<T>())
        .expect("the dialog is open")
}

/// Run frames until `f` holds (a probe or a page fetch answers off the window's thread).
fn until(h: &mut H, what: &str, f: impl Fn(&H) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f(h) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(30));
        h.run();
    }
}

fn no_proxy(h: &mut H) {
    // The golden profile's proxy (127.0.0.1:8080) is not there: connect straight.
    h.state_mut().manager.settings.proxy.clear();
}

#[test]
fn batch_previews_and_adds_to_the_main_queue() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    let before = h.state().manager.downloads.len();
    h.state_mut().run("batch");
    h.run();
    assert!(shows(&h, "Batch download"));
    {
        let b = body::<Batch>(&mut h);
        b.address = String::new();
    }
    h.run();
    assert!(shows(&h, "Put one * in the address"));
    {
        let b = body::<Batch>(&mut h);
        b.address = "https://lid-e2e.invalid/f*.zip".into();
        b.num_from = 1;
        b.num_to = 3;
    }
    h.run();
    assert!(
        shows(&h, "3 files will be added"),
        "batch preview: 3 files will be added"
    );
    assert!(
        shows(&h, "https://lid-e2e.invalid/f3.zip"),
        "the last file shows"
    );
    // Zero fill and letters.
    body::<Batch>(&mut h).width = 3;
    h.run();
    assert!(shows(&h, "https://lid-e2e.invalid/f001.zip"));
    {
        let b = body::<Batch>(&mut h);
        b.letters = true;
        b.let_from = "x".into();
        b.let_to = "z".into();
        assert_eq!(b.urls().len(), 3);
        b.letters = false;
        b.width = 1;
    }
    click(&mut h, "OK");
    assert!(shows(
        &h,
        "3 file(s) were added to the \"Main download queue\". Start the queue now?"
    ));
    click(&mut h, "No");
    assert!(h.state().dialogs.is_empty());
    let m = &h.state().manager;
    assert_eq!(m.downloads.len(), before + 3, "the batch adds 3 downloads");
    assert!(
        m.queues.get(MAIN_QUEUE).is_some_and(|q| !q.running),
        "No leaves the queue stopped"
    );
    assert_eq!(
        m.downloads
            .values()
            .filter(|d| d.url.starts_with("https://lid-e2e.invalid/f") && d.queue == MAIN_QUEUE)
            .count(),
        3
    );
}

#[test]
fn add_an_unreachable_address_later() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    no_proxy(&mut h);
    h.state_mut().run("add");
    h.run();
    assert!(shows(&h, "Enter new address to download"));
    // Not an address: OK does nothing.
    body::<Add>(&mut h).address = "not an address".into();
    click(&mut h, "OK");
    assert_eq!(body::<Add>(&mut h).page, Page::Address);
    body::<Add>(&mut h).address = format!("{CLOSED}/archive.zip");
    click(&mut h, "OK");
    until(&mut h, "the probe", |h| {
        h.query_by_label_contains("Add it anyway?").is_some()
    });
    assert!(
        h.query_by_label_contains("Cannot get information about the file")
            .is_some(),
        "the box says why"
    );
    click(&mut h, "Yes");
    assert!(shows(&h, "Download File Info"));
    assert!(
        shows(&h, "archive.zip"),
        "Download File Info names the file"
    );
    assert_eq!(
        body::<Add>(&mut h).category,
        "Compressed",
        "the category comes from the file type"
    );
    // An invalid checksum holds Start and Later.
    body::<Add>(&mut h).checksum.text = "abc".into();
    h.run();
    let start = h.get_by_role_and_label(Role::Button, "Start Download");
    assert!(
        start.accesskit_node().is_disabled(),
        "an invalid checksum holds Start"
    );
    body::<Add>(&mut h).checksum.text = String::new();
    h.run();
    assert!(
        !h.get_by_role_and_label(Role::Button, "Start Download")
            .accesskit_node()
            .is_disabled()
    );
    click(&mut h, "Download Later");
    // The queue menu lists the queues (the tree shows them too: take the menu's, the last one).
    h.get_all_by_role_and_label(Role::Button, MAIN_QUEUE)
        .last()
        .unwrap()
        .click();
    h.run();
    assert!(h.state().dialogs.is_empty(), "the window closes");
    let d = h
        .state()
        .manager
        .downloads
        .values()
        .find(|d| d.url == format!("{CLOSED}/archive.zip"))
        .cloned()
        .expect("Download Later adds the download");
    assert_eq!(d.queue, MAIN_QUEUE);
    assert_eq!(d.category, "Compressed");
    assert!(
        !h.state().manager.is_running(d.id),
        "later does not start it"
    );
    // The address is remembered for the next time.
    h.state_mut().run("add");
    h.run();
    h.get_by_label("Earlier addresses").click();
    h.run();
    assert!(shows(&h, &format!("{CLOSED}/archive.zip")));
}

#[test]
fn add_it_anyway_no_closes() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    no_proxy(&mut h);
    h.state_mut()
        .requests
        .push(Request::add(&format!("{CLOSED}/nothing.bin")));
    h.state_mut().take_requests();
    h.run();
    until(&mut h, "the probe", |h| {
        h.query_by_label_contains("Add it anyway?").is_some()
    });
    click(&mut h, "No");
    assert!(h.state().dialogs.is_empty());
}

#[test]
fn the_duplicate_rule() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    no_proxy(&mut h);
    let done = h
        .state()
        .manager
        .downloads
        .values()
        .find(|d| d.status == Status::Complete)
        .cloned()
        .unwrap();
    h.state_mut().manager.settings.duplicate_action = "ask".into();
    h.state_mut().requests.push(Request::add(&done.url));
    h.state_mut().take_requests();
    h.run();
    assert!(shows(&h, "Duplicate download link"));
    assert!(shows(&h, "This file is already in your download list."));
    // "If existing file is complete, show download complete dialog", remembered.
    {
        let b = body::<Add>(&mut h);
        b.dup_choice = "complete".into();
        b.dup_remember = true;
    }
    click(&mut h, "OK");
    assert_eq!(h.state().manager.settings.duplicate_action, "complete");
    assert!(
        h.state()
            .dialog_open(&crate::dialogs::complete::label(done.id)),
        "a finished duplicate shows Download complete"
    );
    assert_eq!(h.state().dialogs.len(), 1, "the add window closed");
    // "number": a numbered name in Download File Info.
    h.state_mut()
        .close_dialog(&crate::dialogs::complete::label(done.id));
    h.state_mut().manager.settings.duplicate_action = "number".into();
    h.state_mut().manager.settings.show_complete_dialog = true;
    let local = format!("{CLOSED}/dup.bin");
    let id = {
        let m = &mut h.state_mut().manager;
        m.add(&local, "dup.bin", Default::default()).id
    };
    let dir = h.state().manager.downloads[&id].save_dir.clone();
    h.state_mut().requests.push(Request::add(&local));
    h.state_mut().take_requests();
    h.run();
    until(&mut h, "the probe", |h| {
        h.query_by_label_contains("Add it anyway?").is_some()
    });
    click(&mut h, "Yes");
    assert_eq!(
        body::<Add>(&mut h).path,
        std::path::Path::new(&dir)
            .join("dup (2).bin")
            .to_string_lossy(),
        "the duplicate gets a numbered name"
    );
}

#[test]
fn download_all_links_adds_the_checked_ones() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    no_proxy(&mut h);
    let before = h.state().manager.downloads.len();
    let items = ["a.zip", "b.zip", "c.zip"]
        .iter()
        .map(|n| {
            (
                format!("{CLOSED}/{n}"),
                format!("Link {n}"),
                "a".to_string(),
            )
        })
        .collect();
    crate::dialogs::links::open(
        h.state_mut(),
        items,
        "http://127.0.0.1:9/page.html".into(),
        "http://127.0.0.1:9/page.html".into(),
        Default::default(),
    );
    h.run();
    assert!(shows(&h, "Download All Links"));
    assert!(shows(&h, "3 links found · check the ones to add"));
    for n in ["a.zip", "b.zip", "c.zip"] {
        assert!(shows(&h, n), "the list shows {n}");
    }
    click(&mut h, "Uncheck All");
    assert!(body::<Links>(&mut h).checked.iter().all(|c| !c));
    h.get_by_role_and_label(Role::CheckBox, "a.zip").click();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "c.zip").click();
    h.run();
    assert_eq!(body::<Links>(&mut h).checked, [true, false, true]);
    click(&mut h, "OK");
    assert!(h.state().dialogs.is_empty());
    let m = &h.state().manager;
    assert_eq!(
        m.downloads.len(),
        before + 2,
        "two of three links are added"
    );
    let added: Vec<_> = m
        .downloads
        .values()
        .filter(|d| d.url.starts_with(CLOSED) && d.url.ends_with(".zip"))
        .collect();
    assert!(added.iter().all(|d| d.queue == MAIN_QUEUE));
    assert!(
        added.iter().any(|d| d.description == "Link a.zip"),
        "link texts become descriptions"
    );
    assert!(!added.iter().any(|d| d.url.ends_with("b.zip")));
    assert!(m.queues[MAIN_QUEUE].running, "the Main queue starts");
}

/// A web page with three links, served once.
fn serve_page() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for s in l.incoming().take(2) {
            let Ok(mut s) = s else { continue };
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let html = "<html><body><a href=\"/one.zip\">One</a> <a href=\"two.iso\">Two</a> \
                        <a href=\"/page.html\">A page</a></body></html>";
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                html.len()
            );
        }
    });
    base
}

#[test]
fn the_grabber_lists_the_links_of_a_page() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    no_proxy(&mut h);
    let base = serve_page();
    h.state_mut().run("grabber");
    h.run();
    assert!(shows(&h, "Enter the address of the web page to explore:"));
    body::<Links>(&mut h).grab_url = format!("{base}/index.html");
    click(&mut h, "OK");
    until(&mut h, "the page", |h| shows(h, "one.zip"));
    assert!(shows(&h, "two.iso"));
    assert!(!shows(&h, "page.html"), "HTML files are hidden at first");
    h.get_by_role_and_label(Role::CheckBox, "Hide HTML files")
        .click();
    h.run();
    assert!(shows(&h, "page.html"));
}

#[test]
fn the_grabber_says_when_the_page_cannot_be_opened() {
    let (_tmp, app) = golden();
    let mut h = harness(app);
    no_proxy(&mut h);
    h.state_mut().run("grabber");
    h.run();
    body::<Links>(&mut h).grab_url = format!("{CLOSED}/index.html");
    click(&mut h, "OK");
    until(&mut h, "the note", |h| {
        h.query_by_label_contains("Cannot open the web page")
            .is_some()
    });
    click(&mut h, "OK");
    assert!(h.state().dialogs.is_empty());
}
