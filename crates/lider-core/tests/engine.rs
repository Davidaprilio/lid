//! Port of tests/test_engine.py, plus redirects, FTP and the Range fallback.

mod fixture;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fixture::{FileServer, FtpServer, Options, sha256_hex};
use lider_core::engine::{DownloadTask, EngineOptions};
use lider_core::http::{HttpClient, OpenError};
use lider_core::limiter::SpeedLimiter;
use lider_core::models::{Download, Status};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

struct Env {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    client: HttpClient,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    Env {
        _tmp: tmp,
        dir,
        client: HttpClient::new("lider-test", "", 5.0).unwrap(),
    }
}

impl Env {
    fn options(&self, connections: i64) -> EngineOptions {
        EngineOptions {
            max_connections: connections,
            retries: 3,
            retry_delay: 0.05,
            temp_dir: self.dir.join("tmp").to_string_lossy().into_owned(),
        }
    }

    fn download(&self, url: &str) -> Download {
        Download::new(1, url, "out.bin", &self.dir.join("save").to_string_lossy())
    }

    fn task(
        &self,
        d: Download,
        connections: i64,
        global: Option<Arc<SpeedLimiter>>,
    ) -> (DownloadTask, oneshot::Receiver<()>) {
        let t = DownloadTask::new(
            d,
            self.client.clone(),
            self.options(connections),
            global,
            None,
        );
        let (tx, rx) = oneshot::channel();
        t.on_finish(move || {
            let _ = tx.send(());
        });
        t.start(&Handle::current());
        (t, rx)
    }
}

async fn finished(rx: oneshot::Receiver<()>, secs: u64) {
    tokio::time::timeout(Duration::from_secs(secs), rx)
        .await
        .expect("the task should finish in time")
        .unwrap();
}

fn sha_file(path: &str) -> String {
    sha256_hex(&std::fs::read(path).unwrap())
}

async fn wait_until(mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "condition not reached"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn segmented_download_is_byte_exact() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 5 * 1024 * 1024 + 123,
        delay_per_chunk: Duration::from_millis(2),
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 8, None);
    finished(rx, 30).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(sha_file(&d.path()), srv.sha256);
    assert_eq!(d.size as usize, srv.data().len());
    assert!(d.segments.is_empty());
    assert!(
        t.snapshot().connections.len() > 1,
        "should open several connections"
    );
    let ranged = srv
        .ranges()
        .iter()
        .filter(|r| !r.is_empty() && *r != "bytes=0-")
        .count();
    assert!(ranged > 1, "{:?}", srv.ranges());
    assert!(!Path::new(t.part_path()).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshot_reports_speed_per_connection() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 8 * 1024 * 1024,
        delay_per_chunk: Duration::from_millis(10),
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 4, None);
    wait_until(|| t.snapshot().connections.len() >= 4 && t.download().downloaded >= 1024 * 1024)
        .await;
    let snap = t.snapshot();
    assert_eq!(snap.connections.len(), 4);
    let rates: Vec<f64> = snap.connections.iter().map(|c| c.rate).collect();
    assert!(rates.iter().all(|r| *r > 0.0), "{rates:?}");
    let sum: f64 = rates.iter().sum();
    assert!(
        (sum - snap.rate).abs() <= snap.rate * 0.5,
        "{sum} vs {}",
        snap.rate
    );
    assert!(snap.time_left >= 0);
    assert_eq!(
        snap.connections.iter().map(|c| c.index).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    t.stop();
    finished(rx, 10).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_and_resume() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 4 * 1024 * 1024,
        delay_per_chunk: Duration::from_millis(10),
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 4, None);
    wait_until(|| t.download().downloaded >= 512 * 1024).await;
    t.stop();
    finished(rx, 10).await;
    let d = t.download();
    assert_eq!(d.status, Status::Paused);
    assert!(d.downloaded > 0 && (d.downloaded as usize) < srv.data().len());
    assert!(!d.segments.is_empty());
    assert_eq!(
        d.downloaded,
        d.segments.iter().map(|s| s.pos - s.start).sum::<i64>()
    );

    srv.set_delay(Duration::ZERO);
    srv.clear_requests();
    let (t2, rx2) = e.task(d, 4, None);
    finished(rx2, 30).await;
    let d = t2.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(sha_file(&d.path()), srv.sha256);
    // Resumed connections must not start again from byte 0.
    let ranges = srv.ranges();
    assert!(!ranges[1..].contains(&"bytes=0-".to_string()), "{ranges:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_without_ranges_uses_one_connection() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 1024 * 1024,
        ranges: false,
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 8, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(d.resumable, Some(false));
    assert_eq!(t.snapshot().connections.len(), 1);
    assert_eq!(sha_file(&d.path()), srv.sha256);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_size_streams_to_end() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 700 * 1024,
        ranges: false,
        length: false,
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 8, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(d.size as usize, srv.data().len());
    assert_eq!(sha_file(&d.path()), srv.sha256);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropped_connection_is_retried() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 2 * 1024 * 1024,
        drop_after: 300 * 1024,
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 2, None);
    finished(rx, 30).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(sha_file(&d.path()), srv.sha256);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closed_port_is_an_error() {
    // test_http_404_is_an_error in 1.x points at a closed port.
    let e = env();
    let (t, rx) = e.task(e.download("http://127.0.0.1:9/nothing"), 2, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(d.status, Status::Error);
    assert!(!d.error.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_404_is_fatal_with_its_reason() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 1024,
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&format!("{}/status/404", srv.base)), 2, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(
        (d.status, d.error.as_str()),
        (Status::Error, "HTTP 404 Not Found")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn speed_limit() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 600 * 1024,
        ..Default::default()
    })
    .await;
    let start = Instant::now();
    let (t, rx) = e.task(
        e.download(&srv.url),
        4,
        Some(Arc::new(SpeedLimiter::new(400))),
    );
    finished(rx, 30).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert!(
        start.elapsed() > Duration::from_secs(1),
        "600 KB at 400 KB/s should take over a second"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_file_limit_applies_and_can_change() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 400 * 1024,
        ..Default::default()
    })
    .await;
    let mut d = e.download(&srv.url);
    d.speed_limit_kbps = 200;
    let start = Instant::now();
    let (t, rx) = e.task(d, 4, None);
    finished(rx, 30).await;
    assert_eq!(t.download().status, Status::Complete);
    assert!(start.elapsed() > Duration::from_secs(1));
    t.set_speed_limit(0);
    assert_eq!(t.download().speed_limit_kbps, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn redirects_keep_headers_and_the_name() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 1024 * 1024,
        ..Default::default()
    })
    .await;
    let mut d = e.download(&format!("{}/redirect/2/files/test%20file.bin", srv.base));
    d.headers.insert("Cookie".into(), "session=1".into());
    d.referer = "https://site/".into();
    let probe = e
        .client
        .probe(&d.url, &lider_core::http::request_headers(&d), false)
        .await
        .unwrap();
    assert_eq!(probe.url, srv.url);
    assert_eq!(probe.filename, "test file.bin");
    assert_eq!(probe.size, 1024 * 1024);
    assert!(probe.resumable);
    let (t, rx) = e.task(d, 4, None);
    finished(rx, 20).await;
    assert_eq!(t.download().status, Status::Complete);
    let reqs = srv.inner.requests.lock().unwrap();
    let last = reqs
        .iter()
        .rfind(|r| r.path == "/files/test%20file.bin")
        .unwrap();
    assert_eq!(last.header("cookie"), Some("session=1"));
    assert_eq!(last.header("referer"), Some("https://site/"));
    assert_eq!(last.header("user-agent"), Some("lider-test"));
    assert_eq!(last.header("accept-encoding"), Some("identity"));
    assert_eq!(last.header("accept"), Some("*/*"));
    assert!(last.range.starts_with("bytes="));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn redirect_loop_is_an_error() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 10,
        ..Default::default()
    })
    .await;
    let err = e
        .client
        .open(&format!("{}/loop", srv.base), 0, true, &Default::default())
        .await
        .err()
        .unwrap();
    assert!(
        matches!(err, OpenError::Status { code: 302, .. }),
        "{err:?}"
    );
    assert!(err.message().contains("infinite loop"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forbidden_is_fatal() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 1024,
        fail_first: 1,
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 2, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(
        (d.status, d.error.as_str()),
        (Status::Error, "HTTP 403 Forbidden")
    );
    assert_eq!(srv.ranges().len(), 1, "403 is not retried");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_starts_over_when_the_server_dropped_ranges() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 2 * 1024 * 1024,
        delay_per_chunk: Duration::from_millis(10),
        ..Default::default()
    })
    .await;
    let (t, rx) = e.task(e.download(&srv.url), 2, None);
    wait_until(|| t.download().downloaded >= 256 * 1024).await;
    t.stop();
    finished(rx, 10).await;
    srv.set_ranges(false);
    srv.set_delay(Duration::ZERO);
    // The probe now reports no range support, so the download starts over from 0 as one connection.
    let (t2, rx2) = e.task(t.download(), 2, None);
    finished(rx2, 20).await;
    let d = t2.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(d.resumable, Some(false));
    assert_eq!(sha_file(&d.path()), srv.sha256);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn name_taken_at_finish_gets_a_number() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 1000,
        ..Default::default()
    })
    .await;
    std::fs::create_dir_all(e.dir.join("save")).unwrap();
    std::fs::write(e.dir.join("save/out.bin"), "old").unwrap();
    let (t, rx) = e.task(e.download(&srv.url), 2, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(d.filename, "out (2).bin");
    assert_eq!(
        std::fs::read_to_string(e.dir.join("save/out.bin")).unwrap(),
        "old"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ftp_download() {
    let e = env();
    let srv = FtpServer::start(300 * 1024).await;
    let (t, rx) = e.task(e.download(&srv.url), 8, None);
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(d.size as usize, srv.data.len());
    assert_eq!(d.resumable, Some(false));
    assert_eq!(t.snapshot().connections.len(), 1);
    assert_eq!(sha_file(&d.path()), srv.sha256);
    assert_eq!(srv.logins.lock().unwrap()[0], "anonymous");
    let probe = e
        .client
        .probe(&srv.url, &Default::default(), false)
        .await
        .unwrap();
    assert_eq!(probe.filename, "file one.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn range_ignored_by_a_later_connection_fails_clearly() {
    let e = env();
    let srv = FileServer::start(Options {
        size: 4 * 1024 * 1024,
        delay_per_chunk: Duration::from_millis(5),
        ..Default::default()
    })
    .await;
    srv.ranges_for_next(1); // the probe gets 206, every later request a plain 200
    let (t, rx) = e.task(e.download(&srv.url), 4, None);
    finished(rx, 30).await;
    let d = t.download();
    assert_eq!(
        (d.status, d.error.as_str()),
        (Status::Error, "server ignored the Range header")
    );
    let snap = t.snapshot();
    assert!(
        snap.connections
            .iter()
            .any(|c| c.status == "Error: server ignored the Range header"),
        "{:?}",
        snap.connections
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_answer_times_out() {
    let srv = FileServer::start(Options {
        size: 10,
        ..Default::default()
    })
    .await;
    let client = HttpClient::new("lider-test", "", 0.5).unwrap();
    let start = Instant::now();
    let err = client
        .open(&format!("{}/hang", srv.base), 0, true, &Default::default())
        .await
        .err()
        .unwrap();
    assert_eq!(err.message(), "timed out");
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_proxy_is_used() {
    let e = env();
    let proxy = FileServer::start(Options {
        size: 200 * 1024,
        ..Default::default()
    })
    .await;
    let client = HttpClient::new("lider-test", &proxy.base, 5.0).unwrap();
    let d = Download::new(
        1,
        "http://files.example.invalid/x/test.bin",
        "out.bin",
        &e.dir.join("save").to_string_lossy(),
    );
    let t = DownloadTask::new(d, client, e.options(2), None, None);
    let (tx, rx) = oneshot::channel();
    t.on_finish(move || {
        let _ = tx.send(());
    });
    t.start(&Handle::current());
    finished(rx, 20).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(sha_file(&d.path()), proxy.sha256);
    let reqs = proxy.inner.requests.lock().unwrap();
    assert!(
        reqs.iter()
            .all(|r| r.path.starts_with("http://files.example.invalid/")),
        "went through the proxy"
    );
}
