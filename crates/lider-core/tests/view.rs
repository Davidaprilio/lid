//! The window's list, tree and actions (port of the logic in tests/test_ui.py).

use std::collections::HashSet;

use lider_core::manager::DownloadManager;
use lider_core::models::{Download, MAIN_QUEUE, Status};
use lider_core::paths::Dirs;
use lider_core::search::{FindNext, SearchSettings, flags};
use lider_core::settings::Settings;
use lider_core::store::Store;
use lider_core::view::{self, CheckState, NodeKind, TreeFilter};
use tokio::runtime::Runtime;

fn manager(rt: &Runtime) -> DownloadManager {
    let s = Settings::defaults(&Dirs::from_parts("/home/u".into(), None, None, None));
    DownloadManager::new(s, Store::in_memory().unwrap(), rt.handle().clone()).unwrap()
}

fn put(m: &mut DownloadManager, id: i64, name: &str, category: &str, status: Status) -> Download {
    let mut d = Download::new(
        id,
        &format!("https://x.org/{name}"),
        name,
        "/home/u/Downloads",
    );
    d.category = category.into();
    d.status = status;
    m.downloads.insert(id, d.clone());
    d
}

/// The three downloads of test_ui.py.
fn sample(m: &mut DownloadManager) {
    put(m, 1, "lecture-01.mp4", "Video", Status::Complete);
    let mut z = put(m, 2, "archive.zip", "Compressed", Status::Paused);
    z.size = 100;
    z.downloaded = 25;
    m.downloads.insert(2, z);
    let mut q = put(m, 3, "Lecture-02.mp4", "Video", Status::Queued);
    q.queue = MAIN_QUEUE.into();
    q.queue_order = 1;
    m.downloads.insert(3, q);
}

fn f(kind: NodeKind, value: Option<&str>) -> TreeFilter {
    TreeFilter {
        kind,
        value: value.map(String::from),
    }
}

#[test]
fn category_tree() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let t = view::tree(&m);
    let titles: Vec<&str> = t.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "All Downloads",
            "Unfinished",
            "Finished",
            "Grabber projects",
            "Queues"
        ]
    );
    let all: Vec<&str> = t[0].children.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(
        all,
        ["Compressed", "Documents", "Music", "Programs", "Video"]
    ); // General hidden
    assert!(t[0].expanded && !t[1].expanded && t[4].expanded);
    assert_eq!((t[0].count, t[1].count, t[2].count), (3, 2, 1));
    assert_eq!(t[0].children[4].count, 2); // Video
    assert_eq!(t[0].children[4].icon, "category-video");
    let q: Vec<&str> = t[4].children.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(q, ["Main download queue", "Synchronization queue"]);
    assert_eq!(
        (t[4].children[0].count, t[4].children[1].icon.as_str()),
        (1, "queue-sync")
    );
    assert_eq!(t[3].count, 0);
}

#[test]
fn tree_filters() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let none = SearchSettings::default();
    assert_eq!(view::visible(&m, &f(NodeKind::Finished, None), &none), [1]);
    assert_eq!(
        view::visible(&m, &f(NodeKind::All, Some("Compressed")), &none),
        [2]
    );
    assert_eq!(
        view::visible(&m, &f(NodeKind::Queue, Some(MAIN_QUEUE)), &none),
        [3]
    );
    assert_eq!(view::visible(&m, &f(NodeKind::Queues, None), &none), [3]);
    assert!(view::visible(&m, &f(NodeKind::Grabber, None), &none).is_empty());
    assert_eq!(
        view::visible(&m, &f(NodeKind::Unfinished, Some("Video")), &none),
        [3]
    );
    let nodes = view::tree(&m);
    assert_eq!(
        view::keep_filter(&nodes, &f(NodeKind::Queue, Some("Gone"))),
        TreeFilter::default()
    );
    assert_eq!(
        view::keep_filter(&nodes, &f(NodeKind::Finished, Some("Music"))),
        f(NodeKind::Finished, Some("Music"))
    );
}

#[test]
fn searchbar_filter_and_find_next() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let all = f(NodeKind::All, None);
    let mut s = SearchSettings {
        query: "LECTURE".into(),
        flags: flags::TARGET_NAME,
        active: true,
    };
    assert_eq!(view::visible(&m, &all, &s), [1, 3]); // case-insensitive
    s.active = false; // closing the bar keeps the query but shows everything
    let rows = view::visible(&m, &all, &s);
    assert_eq!(rows, [1, 2, 3]);
    let ds: Vec<Download> = rows.iter().map(|i| m.downloads[i].clone()).collect();
    let mut finder = FindNext::new(s.clone());
    assert_eq!(finder.next(&ds), Ok(Some(0)));
    assert_eq!(finder.next(&ds), Ok(Some(2)));
    // An invalid search (no target) does not filter the list.
    let bad = SearchSettings {
        query: "zip".into(),
        flags: 0,
        active: true,
    };
    assert_eq!(view::visible(&m, &all, &bad), [1, 2, 3]);
}

#[test]
fn sorting_uses_the_1x_keys() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let all = f(NodeKind::All, None);
    let none = SearchSettings::default();
    m.settings.sort_key = "filename".into();
    assert_eq!(view::visible(&m, &all, &none), [2, 1, 3]);
    m.settings.sort_desc = true;
    assert_eq!(view::visible(&m, &all, &none), [3, 1, 2]);
    m.settings.sort_key = "rate".into(); // sorts by bytes downloaded
    m.settings.sort_desc = false;
    assert_eq!(view::visible(&m, &all, &none), [1, 3, 2]);
    m.settings.sort_key = "nonsense".into(); // falls back to the order of addition
    assert_eq!(view::visible(&m, &all, &none), [1, 2, 3]);
    m.settings.sort_key = "status".into();
    assert_eq!(view::visible(&m, &all, &none), [1, 2, 3]); // complete < paused < queued
}

#[test]
fn checked_rows_drive_start_stop_delete() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let visible = vec![1, 2, 3];
    let mut checked: HashSet<i64> = HashSet::new();
    assert_eq!(view::targets(&visible, &checked, &[2]), [2]); // no checks: the selection
    checked.extend([1, 3]);
    assert_eq!(view::targets(&visible, &checked, &[2]), [1, 3]);
    assert_eq!(view::check_state(&visible, &checked), CheckState::Partial);
    assert_eq!(
        view::actions(&m, &view::targets(&visible, &checked, &[]), checked.len()).checked,
        2
    );
    view::toggle_all(&visible, &mut checked);
    assert_eq!(view::check_state(&visible, &checked), CheckState::Checked);
    view::toggle_all(&visible, &mut checked);
    assert_eq!(view::check_state(&visible, &checked), CheckState::Unchecked);
    checked.extend([1, 2, 3]);
    view::prune_checks(&[2], &mut checked);
    assert_eq!(checked, HashSet::from([2]));
}

#[test]
fn action_rules() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let a = view::actions(&m, &[1], 0); // complete
    assert!(!a.resume && !a.stop && a.remove && a.redownload && !a.stop_all && a.delete_completed);
    let a = view::actions(&m, &[2, 3], 0);
    assert!(a.resume && !a.stop);
    let a = view::actions(&m, &[], 0);
    assert!(!a.resume && !a.remove && !a.redownload);
    let mut v = m.downloads[&1].clone();
    v.status = Status::Verifying;
    m.downloads.insert(1, v);
    assert!(
        view::actions(&m, &[1], 0).stop,
        "Stop skips a running check"
    );
}

#[test]
fn status_pills() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    let mut d = put(&mut m, 1, "a.iso", "General", Status::Complete);
    d.checksum = "sha256:00".into();
    d.checksum_result = "verified".into();
    let p = view::status_pill(&m, &d);
    assert_eq!((p.kind, p.text.as_str()), ("verified", "Verified"));
    d.checksum_result = "mismatch".into();
    assert_eq!(view::status_pill(&m, &d).text, "Checksum mismatch");
    d.checksum_result.clear();
    assert_eq!(view::status_pill(&m, &d).text, "Complete");
    d.status = Status::Error;
    d.error = "HTTP 404 Not Found".into();
    assert_eq!(view::status_pill(&m, &d).text, "Error 404");
    d.error = "timed out".into();
    assert_eq!(view::status_pill(&m, &d).text, "Error");
    d.status = Status::Paused;
    d.size = 200;
    d.downloaded = 50;
    let p = view::status_pill(&m, &d);
    assert_eq!(
        (p.kind, p.text.as_str(), p.progress),
        ("pause", "25.0% · Paused", 0.25)
    );
    d.size = -1;
    d.downloaded = 2048;
    assert_eq!(view::status_pill(&m, &d).text, "2.00 KB · Paused");
    d.downloaded = 0;
    d.status = Status::Queued;
    assert_eq!(view::status_pill(&m, &d).text, "Queued");
    d.status = Status::Verifying;
    assert_eq!(view::status_pill(&m, &d).text, "Verifying 0%");
}

#[test]
fn rows_cells_and_columns() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let cols = view::columns(&m);
    assert_eq!(
        cols,
        [
            "queue",
            "filename",
            "size",
            "status",
            "time_left",
            "rate",
            "last_try",
            "description"
        ]
    );
    let rows = view::rows(&m, &[3, 2], &HashSet::from([2]), &|_| None);
    assert_eq!(rows[0].queue, MAIN_QUEUE);
    assert_eq!(rows[0].cells[0], ""); // the Q column draws an icon only
    assert_eq!(rows[0].cells[1], "Lecture-02.mp4");
    assert_eq!(rows[0].cells[3], ""); // the status column draws a pill
    assert!(rows[1].checked && !rows[0].checked);
    assert_eq!(rows[1].icon, "category-compressed");
    assert_eq!(view::column_title("time_left"), "Time left");
    assert_eq!(
        view::cell_text(&m.downloads[&2], "time_left", Some(25.0)),
        "3 sec"
    );
    assert_eq!(
        view::cell_text(&m.downloads[&2], "rate", Some(2048.0)),
        "2.00 KB/sec"
    );
    assert_eq!(
        view::cell_text(&m.downloads[&2], "save_to", None),
        "/home/u/Downloads/archive.zip"
    );
}

#[test]
fn status_bar_and_tray() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let s = view::status_bar(&m, 0.0);
    assert_eq!(s.speed, "↓ 0 B/s");
    assert_eq!(s.counts, "0 downloading · 1 in queue");
    assert_eq!(
        (s.limiter.as_str(), s.clipboard.as_str()),
        ("Speed Limiter: off", "Clipboard: on")
    );
    assert_eq!(view::status_bar(&m, 3072.0).speed, "↓ 3.00 KB/sec");
    assert_eq!(view::tray_tooltip(&m), "LID");
}

#[test]
fn details_pane() {
    let rt = Runtime::new().unwrap();
    let mut m = manager(&rt);
    sample(&mut m);
    let d = view::details(&m, &m.downloads[&2], None);
    assert_eq!(d.url, "x.org/archive.zip");
    assert_eq!(d.state, "Paused · Compressed");
    assert_eq!((d.action, d.action_label), ("resume", "Resume"));
    assert_eq!(
        (d.percent.as_str(), d.of.as_str()),
        ("25.0%", "25 Bytes of 100 Bytes")
    );
    assert_eq!(
        (d.rate.as_str(), d.connections.as_str(), d.resume),
        ("—", "—", "Unknown")
    );
    assert_eq!(d.done, "25 / 100 Bytes");
    assert_eq!(d.checksum, "—");
    assert_eq!((d.limit_on, d.limit_kbps), (false, 200));
    let c = view::details(&m, &m.downloads[&1], None);
    assert_eq!((c.action, c.state.as_str()), ("open", "Complete · Video"));
    assert_eq!(view::of(2_566_914_048, 6_098_812_928), "2.39 / 5.68 GB");
    assert_eq!(view::of(512, 6_098_812_928), "512 Bytes / 5.68 GB");
}
