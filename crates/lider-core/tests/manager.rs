//! Port of the manager cases in tests/test_manager.py, plus queues, scheduler and restart details.

mod fixture;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use fixture::{FileServer, Options};
use lider_core::manager::{DownloadManager, Event, EventKind, NewDownload};
use lider_core::models::{Download, MAIN_QUEUE, SYNC_QUEUE, Status};
use lider_core::paths::Dirs;
use lider_core::settings::Settings;
use lider_core::store::Store;
use tokio::runtime::Runtime;

struct T {
    rt: Runtime,
    _tmp: tempfile::TempDir,
    root: PathBuf,
    srv: FileServer,
    m: DownloadManager,
}

fn setup() -> T {
    let rt = Runtime::new().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let mut s = Settings::defaults(&Dirs::from_parts(root.clone(), None, None, None));
    s.temp_dir = root.join("parts").to_string_lossy().into_owned();
    s.retry_delay = 0.05;
    for c in &mut s.categories {
        c.save_dir = root.join("dl").join(&c.name).to_string_lossy().into_owned();
    }
    let store = Store::open(&root.join("db.sqlite")).unwrap();
    let m = DownloadManager::new(s, store, rt.handle().clone()).unwrap();
    let srv = rt.block_on(FileServer::start(Options {
        size: 1024 * 1024,
        ..Default::default()
    }));
    T {
        rt,
        _tmp: tmp,
        root,
        srv,
        m,
    }
}

impl T {
    fn wait_for(&mut self, mut pred: impl FnMut(&DownloadManager) -> bool) -> Vec<Event> {
        let end = Instant::now() + Duration::from_secs(30);
        let mut events = vec![];
        while Instant::now() < end {
            events.extend(self.m.poll(None));
            if pred(&self.m) {
                return events;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out; events so far: {events:?}");
    }

    fn add(&mut self, name: &str, o: NewDownload) -> i64 {
        let url = self.srv.url.clone();
        self.m.add(&url, name, o).id
    }

    fn d(&self, id: i64) -> Download {
        self.m.downloads[&id].clone()
    }
}

impl Drop for T {
    fn drop(&mut self) {
        self.m.shutdown();
        let _ = &self.rt;
    }
}

fn kinds(events: &[Event], id: i64) -> Vec<EventKind> {
    events
        .iter()
        .filter(|e| e.download_id == Some(id) && e.kind != EventKind::Changed)
        .map(|e| e.kind)
        .collect()
}

#[test]
fn default_queues_exist() {
    let t = setup();
    assert!(t.m.queues.contains_key(MAIN_QUEUE));
    assert!(t.m.queues[SYNC_QUEUE].schedule.periodic);
    assert!(!t.m.queues[MAIN_QUEUE].schedule.periodic);
}

#[test]
fn add_classifies_and_completes() {
    let mut t = setup();
    let id = t.add(
        "movie.mkv",
        NewDownload {
            start: true,
            ..Default::default()
        },
    );
    assert_eq!(t.d(id).category, "Video");
    let events = t.wait_for(|m| m.downloads[&id].status == Status::Complete);
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::Complete && e.download_id == Some(id))
    );
    let d = t.d(id);
    assert!(Path::new(&d.path()).exists());
    assert!(
        d.path()
            .starts_with(&t.m.settings.category("Video").save_dir)
    );
}

#[test]
fn checksum_is_verified_before_complete() {
    let mut t = setup();
    let cs = format!("sha256:{}", t.srv.sha256);
    let id = t.add(
        "a.iso",
        NewDownload {
            checksum: cs,
            start: true,
            ..Default::default()
        },
    );
    let events = t.wait_for(|m| {
        let d = &m.downloads[&id];
        d.status == Status::Complete && !d.checksum_result.is_empty()
    });
    let d = t.d(id);
    assert_eq!(d.checksum_result, "verified");
    assert_eq!(d.checksum_actual, t.srv.sha256);
    assert_eq!(kinds(&events, id), [EventKind::Complete]);
}

#[test]
fn checksum_mismatch_asks_keeps_or_downloads_again_once() {
    let mut t = setup();
    let wrong = format!("sha256:{}", "0".repeat(64));
    let id = t.add(
        "a.iso",
        NewDownload {
            checksum: wrong,
            start: true,
            ..Default::default()
        },
    );
    let events = t.wait_for(|m| m.downloads[&id].checksum_result == "mismatch");
    assert!(events.iter().any(|e| e.download_id == Some(id)
        && e.kind == EventKind::Mismatch
        && e.message.is_empty()));
    assert_eq!(t.d(id).status, Status::Complete);
    assert!(!kinds(&events, id).contains(&EventKind::Complete));

    t.m.settings.checksum_mismatch = "redownload".into();
    let requests = t.srv.ranges().len();
    t.m.redownload(id, true, true);
    let events = t.wait_for(|m| {
        let d = &m.downloads[&id];
        d.checksum_retried && d.checksum_result == "mismatch"
    });
    assert_eq!(kinds(&events, id), [EventKind::Mismatch]); // downloaded again once by itself, then asked
    assert!(t.srv.ranges().len() > requests + 1);
}

#[test]
fn mismatch_kept_says_so() {
    let mut t = setup();
    t.m.settings.checksum_mismatch = "keep".into();
    let wrong = format!("md5:{}", "0".repeat(32));
    let id = t.add(
        "a.iso",
        NewDownload {
            checksum: wrong,
            start: true,
            ..Default::default()
        },
    );
    let events = t.wait_for(|m| m.downloads[&id].checksum_result == "mismatch");
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::Mismatch && e.message == "kept")
    );
}

#[test]
fn skip_check_and_verify_now() {
    let mut t = setup();
    t.m.settings.verify_checksum = false;
    let cs = format!("sha256:{}", t.srv.sha256);
    let id = t.add(
        "a.iso",
        NewDownload {
            checksum: cs,
            start: true,
            ..Default::default()
        },
    );
    t.wait_for(|m| m.downloads[&id].status == Status::Complete);
    assert_eq!(t.d(id).checksum_result, ""); // verification turned off in Options
    assert!(t.m.verify(id, true));
    let events = t.wait_for(|m| {
        let d = &m.downloads[&id];
        d.status == Status::Complete && !d.checksum_result.is_empty()
    });
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::Checked && e.message == "verified")
    );
    assert!(!events.iter().any(|e| e.kind == EventKind::Complete));
    assert!(!(t.m.start(id).is_some() && t.m.is_running(id)));
}

#[test]
fn check_when_finished_runs_even_with_verification_off() {
    let mut t = setup();
    t.m.settings.verify_checksum = false;
    let id = t.add("a.iso", NewDownload::default());
    let cs = format!("sha256:{}", t.srv.sha256);
    t.m.check_when_finished(id, &cs);
    t.m.start(id);
    let events = t.wait_for(|m| {
        let d = &m.downloads[&id];
        d.status == Status::Complete && !d.checksum_result.is_empty()
    });
    assert_eq!(t.d(id).checksum_result, "verified");
    assert!(kinds(&events, id).contains(&EventKind::Complete));
}

#[test]
fn queue_runs_items_one_at_a_time_in_order() {
    let mut t = setup();
    let ids: Vec<i64> = (0..3)
        .map(|i| t.add(&format!("f{i}.zip"), NewDownload::default()))
        .collect();
    t.m.set_queue(&ids, MAIN_QUEUE);
    t.m.move_in_queue(ids[2], -2);
    let order: Vec<i64> = t.m.queue_items(MAIN_QUEUE).iter().map(|d| d.id).collect();
    assert_eq!(order, [ids[2], ids[0], ids[1]]);
    t.m.start_queue(MAIN_QUEUE);
    let mut running_seen = 0;
    let end = Instant::now() + Duration::from_secs(30);
    let mut events = vec![];
    while Instant::now() < end {
        events.extend(t.m.poll(None));
        running_seen = running_seen.max(ids.iter().filter(|i| t.m.is_running(**i)).count());
        if events.iter().any(|e| e.kind == EventKind::QueueDone) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ids.iter().all(|i| t.d(*i).status == Status::Complete));
    assert!(running_seen <= 1);
    assert!(!t.m.queues[MAIN_QUEUE].running);
}

#[test]
fn queue_runs_n_at_a_time() {
    let mut t = setup();
    t.srv.set_delay(Duration::from_millis(5));
    let ids: Vec<i64> = (0..4)
        .map(|i| t.add(&format!("f{i}.zip"), NewDownload::default()))
        .collect();
    t.m.set_queue(&ids, MAIN_QUEUE);
    t.m.queues
        .get_mut(MAIN_QUEUE)
        .unwrap()
        .schedule
        .max_parallel = 2;
    let started = t.m.start_queue(MAIN_QUEUE);
    assert_eq!(started[0].kind, EventKind::QueueStarted);
    assert_eq!(ids.iter().filter(|i| t.m.is_running(**i)).count(), 2);
    t.wait_for(|m| !m.queues[MAIN_QUEUE].running);
    assert!(ids.iter().all(|i| t.d(*i).status == Status::Complete));
}

#[test]
fn scheduler_starts_and_stops_queue() {
    let mut t = setup();
    let id = t.add("a.zip", NewDownload::default());
    t.m.set_queue(&[id], MAIN_QUEUE);
    {
        let s = &mut t.m.queues.get_mut(MAIN_QUEUE).unwrap().schedule;
        s.start_enabled = true;
        s.start_time = "03:15".into();
        s.daily = true;
        s.days = vec![0];
        s.stop_enabled = true;
        s.stop_time = "03:16".into();
    }
    let at = |d: u32, h: u32, mi: u32| {
        NaiveDate::from_ymd_opt(2026, 10, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    };
    t.m.poll(Some(at(20, 3, 15))); // Tuesday: not selected
    assert!(!t.m.queues[MAIN_QUEUE].running);
    t.srv.set_delay(Duration::from_millis(50)); // slow enough to still run at 03:16
    let events = t.m.poll(Some(at(19, 3, 15))); // Monday
    assert!(events.iter().any(|e| e.kind == EventKind::QueueStarted));
    assert!(t.m.queues[MAIN_QUEUE].running);
    assert!(t.m.is_running(id));
    // Same minute again: fires once only.
    let again = t.m.poll(Some(at(19, 3, 15)));
    assert!(!again.iter().any(|e| e.kind == EventKind::QueueStarted));
    let stop = t.m.poll(Some(at(19, 3, 16)));
    assert!(
        stop.iter()
            .any(|e| e.kind == EventKind::QueueDone && e.message == "stopped by schedule")
    );
    assert!(!t.m.queues[MAIN_QUEUE].running);
    t.wait_for(|m| !m.is_running(id));
    assert_eq!(t.d(id).status, Status::Paused);
}

#[test]
fn once_date_and_start_on_startup() {
    let mut t = setup();
    let id = t.add("a.zip", NewDownload::default());
    t.m.set_queue(&[id], SYNC_QUEUE);
    {
        let s = &mut t.m.queues.get_mut(MAIN_QUEUE).unwrap().schedule;
        s.start_enabled = true;
        s.start_time = "08:00".into();
        s.once_date = "2026-12-24".into();
    }
    t.m.queues
        .get_mut(SYNC_QUEUE)
        .unwrap()
        .schedule
        .start_on_startup = true;
    let at = |d: u32| {
        NaiveDate::from_ymd_opt(2026, 12, d)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
    };
    let first = t.m.poll(Some(at(23)));
    // Start on startup fires on the first poll; the once date is not today.
    let started: Vec<_> = first
        .iter()
        .filter(|e| e.kind == EventKind::QueueStarted)
        .map(|e| e.queue.clone().unwrap())
        .collect();
    assert_eq!(started, [SYNC_QUEUE]);
    let second = t.m.poll(Some(at(24)));
    assert!(
        second
            .iter()
            .any(|e| e.kind == EventKind::QueueStarted && e.queue.as_deref() == Some(MAIN_QUEUE))
    );
}

#[test]
fn periodic_queue_downloads_again_and_plans_the_next_run() {
    let mut t = setup();
    let id = t.add("a.zip", NewDownload::default());
    t.m.set_queue(&[id], SYNC_QUEUE);
    t.m.start_queue(SYNC_QUEUE);
    t.wait_for(|m| !m.queues[SYNC_QUEUE].running);
    assert_eq!(t.d(id).status, Status::Complete);
    let q = &t.m.queues[SYNC_QUEUE];
    let expected = lider_core::models::now() + 2.0 * 3600.0;
    assert!((q.next_run - expected).abs() < 5.0, "{}", q.next_run);
    // Starting again re-downloads the finished file.
    let requests = t.srv.ranges().len();
    t.m.start_queue(SYNC_QUEUE);
    assert!(t.m.is_running(id) || t.d(id).status != Status::Complete);
    t.wait_for(|m| !m.queues[SYNC_QUEUE].running);
    assert!(t.srv.ranges().len() > requests);
}

#[test]
fn failed_queue_items_are_retried_up_to_the_limit() {
    let mut t = setup();
    let id =
        t.m.add(
            "http://127.0.0.1:9/nothing",
            "x.zip",
            NewDownload::default(),
        )
        .id;
    t.m.set_queue(&[id], MAIN_QUEUE);
    {
        let s = &mut t.m.queues.get_mut(MAIN_QUEUE).unwrap().schedule;
        s.retries = 2;
    }
    t.m.start_queue(MAIN_QUEUE);
    let events = t.wait_for(|m| !m.queues[MAIN_QUEUE].running);
    let errors = events.iter().filter(|e| e.kind == EventKind::Error).count();
    assert_eq!(errors, 3, "first try plus 2 retries");
    assert_eq!(t.m.queues[MAIN_QUEUE].failures[&id], 3);
}

#[test]
fn remove_and_persist() {
    let mut t = setup();
    let id = t.add(
        "doc.pdf",
        NewDownload {
            description: "hello".into(),
            ..Default::default()
        },
    );
    let again = Store::open(&t.root.join("db.sqlite")).unwrap();
    assert_eq!(again.load_downloads().unwrap().0[0].description, "hello");
    t.m.remove(&[id], false);
    assert!(t.m.store.load_downloads().unwrap().0.is_empty());
}

#[test]
fn remove_deletes_parts_and_finished_files_when_asked() {
    let mut t = setup();
    let a = t.add(
        "a.bin",
        NewDownload {
            start: true,
            ..Default::default()
        },
    );
    t.wait_for(|m| m.downloads[&a].status == Status::Complete);
    let path = t.d(a).path();
    t.m.remove(&[a], true);
    assert!(!Path::new(&path).exists());
    let b = t.add("b.bin", NewDownload::default());
    let part = t.root.join("parts").join(format!("{b}.part"));
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, "x").unwrap();
    t.m.remove(&[b], false);
    assert!(!part.exists());
}

#[test]
fn active_status_becomes_paused_after_restart() {
    let t = setup();
    let mut d = Download::new(7, "http://x/y", "y", "/tmp");
    d.status = Status::Downloading;
    t.m.store.save_download(&d).unwrap();
    let mut v = Download::new(8, "http://x/z", "z", "/tmp");
    v.status = Status::Verifying;
    t.m.store.save_download(&v).unwrap();
    let store = Store::open(&t.root.join("db.sqlite")).unwrap();
    let m2 = DownloadManager::new(t.m.settings.clone(), store, t.rt.handle().clone()).unwrap();
    assert_eq!(m2.downloads[&7].status, Status::Paused);
    assert_eq!(m2.downloads[&8].status, Status::Complete);
}

#[test]
fn pause_resume_and_edit_while_running() {
    let mut t = setup();
    // Slow enough that the stop arrives before the end, also on a fast Windows runner.
    t.srv.set_delay(Duration::from_millis(60));
    let id = t.add(
        "slow.bin",
        NewDownload {
            start: true,
            ..Default::default()
        },
    );
    t.wait_for(|m| m.downloads[&id].downloaded > 64 * 1024);
    t.m.edit(id, |d| d.description = "changed while running".into());
    t.m.set_download_speed_limit(id, 500);
    t.m.stop(id);
    let events = t.wait_for(|m| !m.is_running(id));
    let d = t.d(id);
    assert_eq!(d.status, Status::Paused);
    assert_eq!(d.description, "changed while running");
    assert_eq!(d.speed_limit_kbps, 500);
    assert!(kinds(&events, id).is_empty() || events.iter().any(|e| e.kind == EventKind::Changed));
    t.srv.set_delay(Duration::ZERO);
    t.m.set_download_speed_limit(id, 0);
    t.m.start(id);
    t.wait_for(|m| m.downloads[&id].status == Status::Complete);
    assert_eq!(
        fixture::sha256_hex(&std::fs::read(t.d(id).path()).unwrap()),
        t.srv.sha256
    );
}

#[test]
fn queues_are_saved_and_deleted() {
    let mut t = setup();
    t.m.add_queue("Nightly");
    t.m.queues.get_mut("Nightly").unwrap().schedule.max_parallel = 3;
    t.m.save_queue("Nightly");
    let id = t.add(
        "a.zip",
        NewDownload {
            queue: "Nightly".into(),
            ..Default::default()
        },
    );
    assert_eq!(t.d(id).queue_order, 1);
    let (saved, _) = t.m.store.load_queues().unwrap();
    assert_eq!(saved["Nightly"].schedule.max_parallel, 3);
    t.m.delete_queue(MAIN_QUEUE); // built-in: kept
    assert!(t.m.queues.contains_key(MAIN_QUEUE));
    t.m.delete_queue("Nightly");
    assert!(!t.m.queues.contains_key("Nightly"));
    assert_eq!(t.d(id).queue, "");
    assert!(!t.m.store.load_queues().unwrap().0.contains_key("Nightly"));
}

#[test]
fn remove_completed_and_stop_all() {
    let mut t = setup();
    let a = t.add(
        "a.bin",
        NewDownload {
            start: true,
            ..Default::default()
        },
    );
    t.wait_for(|m| m.downloads[&a].status == Status::Complete);
    t.srv.set_delay(Duration::from_millis(50));
    let b = t.add(
        "b.bin",
        NewDownload {
            start: true,
            ..Default::default()
        },
    );
    t.m.queues.get_mut(MAIN_QUEUE).unwrap().running = true;
    t.m.stop_all();
    assert!(!t.m.queues[MAIN_QUEUE].running);
    t.wait_for(|m| !m.is_running(b));
    assert_eq!(t.d(b).status, Status::Paused);
    assert_eq!(t.m.remove_completed(), vec![a]);
    assert!(t.m.downloads.contains_key(&b));
}

#[test]
fn bad_proxy_fails_the_download_with_a_message() {
    let mut t = setup();
    t.m.settings.proxy = "::bad::".into();
    let id = t.add(
        "a.bin",
        NewDownload {
            start: true,
            ..Default::default()
        },
    );
    let events = t.m.poll(None);
    assert_eq!(t.d(id).status, Status::Error);
    assert!(t.d(id).error.starts_with("Bad proxy address"));
    assert!(events.iter().any(|e| e.kind == EventKind::Error));
}

#[test]
fn global_limiter_follows_the_setting() {
    let mut t = setup();
    t.m.set_speed_limiter(true, Some(300));
    assert_eq!(t.m.limiter.rate(), 300 * 1024);
    assert!(t.m.settings.speed_limiter_on);
    t.m.set_speed_limiter(false, None);
    assert_eq!(t.m.limiter.rate(), 0);
    assert_eq!(t.m.settings.speed_limit_kbps, 300);
}

#[test]
fn youtube_download_through_the_manager() {
    use lider_core::media::{self, MediaInfo, Resolved, Variant};
    if media::which("ffmpeg").is_none() {
        eprintln!("skipped: ffmpeg is required");
        return;
    }
    let mut t = setup();
    let clip = t.root.join("a.m4a");
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:a",
            "aac",
        ])
        .arg(&clip)
        .status()
        .unwrap();
    assert!(ok.success());
    let audio = t.rt.block_on(FileServer::start(Options {
        data: Some(std::fs::read(&clip).unwrap()),
        ..Default::default()
    }));
    let url = audio.url.clone();
    t.m.set_resolver(std::sync::Arc::new(move |_: &str, ids: &[String]| {
        Ok(ids
            .iter()
            .map(|_| Resolved {
                chunk: 0,
                url: url.clone(),
                headers: Default::default(),
                size: -1,
                ext: "m4a".into(),
            })
            .collect())
    }));
    let v = Variant {
        audio_format: "140".into(),
        ..Variant::new("audio", "M4A", "M4A", "m4a")
    };
    let info = MediaInfo {
        url: "https://youtu.be/x".into(),
        id: "x".into(),
        title: "Tone".into(),
        uploader: String::new(),
        duration: 2,
        variants: vec![v.clone()],
    };
    let id =
        t.m.add(
            &info.url,
            &media::suggested_filename(&info, &v),
            NewDownload {
                media: Some(media::media_record(&info, &v)),
                start: true,
                ..Default::default()
            },
        )
        .id;
    assert!(matches!(
        t.m.task(id),
        Some(lider_core::manager::Task::Media(_))
    ));
    let events = t.wait_for(|m| m.downloads[&id].status == Status::Complete);
    assert!(kinds(&events, id).contains(&EventKind::Complete));
    assert_eq!(t.d(id).filename, "Tone.m4a");
    assert_eq!(t.d(id).category, "Music");
    // Download again: the stream progress is forgotten and the file is fetched once more.
    t.m.redownload(id, true, true);
    let streams = &t.m.downloads[&id].media.as_ref().unwrap()["streams"][0];
    assert!(streams.get("done").is_none() || t.m.is_running(id));
    t.wait_for(|m| m.downloads[&id].status == Status::Complete);
    assert!(Path::new(&t.d(id).path()).exists());
}
