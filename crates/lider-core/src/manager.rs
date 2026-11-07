//! Owns the download list, running tasks, queues and the scheduler (`manager.py` in 1.x).
//!
//! The app calls [`DownloadManager::poll`] on a 500 ms timer; finished tasks, finished checks, queue
//! progress and scheduled starts and stops are handled there, so worker tasks never call into the UI.
//!
//! In 1.x the manager and a running task shared one `Download` object. Here a running task holds its own
//! copy, which is the truth while it runs: `poll()` copies it into the list, and edits made while it runs
//! go to both ([`DownloadManager::edit`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveDateTime};
use indexmap::IndexMap;
use serde_json::{Map, Value};
use tokio::runtime::Handle;

use crate::categories;
use crate::checksum::{CheckResult, Verifier};
use crate::engine::{DownloadTask, EngineOptions, Snapshot};
use crate::http::{HttpClient, OpenError, ProbeResult, request_headers};
use crate::limiter::SpeedLimiter;
use crate::media_task::{MediaTask, Resolver, remove_parts, reset_streams};
use crate::models::{Download, MAIN_QUEUE, SYNC_QUEUE, Status, now};
use crate::pyutil::url_hostname;
use crate::queue::{QueueRecord, QueueSchedule};
use crate::settings::Settings;
use crate::store::{BadRow, Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    Complete,
    Error,
    Mismatch,
    Checked,
    QueueStarted,
    QueueDone,
    Changed,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Complete => "complete",
            EventKind::Error => "error",
            EventKind::Mismatch => "mismatch",
            EventKind::Checked => "checked",
            EventKind::QueueStarted => "queue_started",
            EventKind::QueueDone => "queue_done",
            EventKind::Changed => "changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub download_id: Option<i64>,
    pub queue: Option<String>,
    pub message: String,
}

impl Event {
    fn download(kind: EventKind, id: i64, message: impl Into<String>) -> Event {
        Event {
            kind,
            download_id: Some(id),
            queue: None,
            message: message.into(),
        }
    }

    fn queue(kind: EventKind, name: &str, message: impl Into<String>) -> Event {
        Event {
            kind,
            download_id: None,
            queue: Some(name.into()),
            message: message.into(),
        }
    }
}

/// A queue with its running state.
#[derive(Debug, Clone, PartialEq)]
pub struct Queue {
    pub name: String,
    pub schedule: QueueSchedule,
    pub running: bool,
    /// Wall-clock time of the next periodic start, 0 = none.
    pub next_run: f64,
    /// Errors per download since the queue started.
    pub failures: HashMap<i64, i64>,
    fired: HashSet<String>,
    /// Fields of the saved record written by another LID version.
    extra: Map<String, Value>,
}

impl Queue {
    pub fn new(name: &str, schedule: QueueSchedule) -> Queue {
        Queue {
            name: name.into(),
            schedule,
            running: false,
            next_run: 0.0,
            failures: HashMap::new(),
            fired: HashSet::new(),
            extra: Map::new(),
        }
    }
}

/// A running download: plain or YouTube.
#[derive(Clone)]
pub enum Task {
    Plain(Arc<DownloadTask>),
    Media(Arc<MediaTask>),
}

impl Task {
    pub fn download(&self) -> Download {
        match self {
            Task::Plain(t) => t.download(),
            Task::Media(t) => t.download(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        match self {
            Task::Plain(t) => t.snapshot(),
            Task::Media(t) => t.snapshot(),
        }
    }

    pub fn is_running(&self) -> bool {
        match self {
            Task::Plain(t) => t.is_running(),
            Task::Media(t) => t.is_running(),
        }
    }

    pub fn stop(&self) {
        match self {
            Task::Plain(t) => t.stop(),
            Task::Media(t) => t.stop(),
        }
    }

    pub fn set_speed_limit(&self, kbps: i64) {
        match self {
            Task::Plain(t) => t.set_speed_limit(kbps),
            Task::Media(t) => t.set_speed_limit(kbps),
        }
    }

    pub fn error(&self) -> String {
        match self {
            Task::Plain(t) => t.error(),
            Task::Media(t) => t.error(),
        }
    }

    /// The phase text of a YouTube download, "" otherwise.
    pub fn status_text(&self) -> String {
        match self {
            Task::Plain(_) => String::new(),
            Task::Media(t) => t.status_text(),
        }
    }

    fn update(&self, f: &mut dyn FnMut(&mut Download)) {
        match self {
            Task::Plain(t) => t.update_download(f),
            Task::Media(t) => t.update_download(f),
        }
    }

    /// Wait (blocking) until the task has ended or `timeout` passed.
    pub fn wait(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while self.is_running() {
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }
}

struct Running {
    task: Task,
    serial: u64,
}

/// Fields the user may change while a download runs; the engine owns the rest.
fn copy_user_fields(to: &mut Download, from: &Download) {
    to.url = from.url.clone();
    to.filename = from.filename.clone();
    to.save_dir = from.save_dir.clone();
    to.category = from.category.clone();
    to.queue = from.queue.clone();
    to.queue_order = from.queue_order;
    to.description = from.description.clone();
    to.referer = from.referer.clone();
    to.parent_page = from.parent_page.clone();
    to.username = from.username.clone();
    to.password = from.password.clone();
    to.headers = from.headers.clone();
    to.connections = from.connections;
    to.checksum = from.checksum.clone();
    to.checksum_result = from.checksum_result.clone();
    to.checksum_actual = from.checksum_actual.clone();
    to.checksum_checked = from.checksum_checked;
    to.checksum_seconds = from.checksum_seconds;
    to.checksum_retried = from.checksum_retried;
    to.checksum_force = from.checksum_force;
    to.extra = from.extra.clone();
}

/// What `add` needs besides the URL and file name; the defaults match 1.x.
#[derive(Debug, Clone, Default)]
pub struct NewDownload {
    pub save_dir: Option<String>,
    pub category: Option<String>,
    pub description: String,
    pub referer: String,
    pub parent_page: String,
    pub username: String,
    pub password: String,
    pub size: Option<i64>,
    pub resumable: Option<bool>,
    pub queue: String,
    pub start: bool,
    pub media: Option<Map<String, Value>>,
    pub headers: IndexMap<String, String>,
    pub checksum: String,
}

pub struct DownloadManager {
    pub settings: Settings,
    pub store: Store,
    /// Every download by id. Entries of running downloads are refreshed from their task on `poll()`.
    pub downloads: BTreeMap<i64, Download>,
    pub queues: IndexMap<String, Queue>,
    /// The global speed limiter, shared by every task.
    pub limiter: Arc<SpeedLimiter>,
    /// Rows of downloads.db that could not be read (left in the file).
    pub bad_rows: Vec<BadRow>,
    rt: Handle,
    tasks: HashMap<i64, Running>,
    serial: u64,
    finished: Arc<Mutex<Vec<(i64, u64)>>>,
    verifiers: HashMap<i64, Arc<Verifier>>,
    verified_tx: Sender<i64>,
    verified_rx: Receiver<i64>,
    pending: Vec<Event>,
    next_id: i64,
    started: bool,
    last_save: Instant,
    resolver: Option<Resolver>,
    ffmpeg: Option<Option<std::path::PathBuf>>,
}

impl DownloadManager {
    /// Load the list and queues. A download that was running when LID closed becomes Paused; one that
    /// was being checked becomes Complete (not checked). Tasks run on `rt`.
    pub fn new(
        settings: Settings,
        store: Store,
        rt: Handle,
    ) -> Result<DownloadManager, StoreError> {
        let (loaded, mut bad_rows) = store.load_downloads()?;
        let mut downloads = BTreeMap::new();
        for mut d in loaded {
            if d.status.is_active() {
                d.status = Status::Paused; // the app was closed while downloading
            } else if d.status == Status::Verifying {
                d.status = Status::Complete; // closed while checking: complete, not checked
            }
            downloads.insert(d.id, d);
        }
        let (records, bad_queues) = store.load_queues()?;
        bad_rows.extend(bad_queues);
        let mut queues = IndexMap::new();
        for (name, r) in records {
            let mut q = Queue::new(&name, r.schedule);
            q.extra = r.extra;
            queues.insert(name, q);
        }
        for (name, periodic) in [(MAIN_QUEUE, false), (SYNC_QUEUE, true)] {
            if !queues.contains_key(name) {
                queues.insert(
                    name.to_string(),
                    Queue::new(
                        name,
                        QueueSchedule {
                            periodic,
                            ..Default::default()
                        },
                    ),
                );
            }
        }
        let limiter = Arc::new(SpeedLimiter::new(if settings.speed_limiter_on {
            settings.speed_limit_kbps
        } else {
            0
        }));
        let next_id = store.next_id()?;
        let (verified_tx, verified_rx) = channel();
        Ok(DownloadManager {
            settings,
            store,
            downloads,
            queues,
            limiter,
            bad_rows,
            rt,
            tasks: HashMap::new(),
            serial: 0,
            finished: Arc::new(Mutex::new(Vec::new())),
            verifiers: HashMap::new(),
            verified_tx,
            verified_rx,
            pending: Vec::new(),
            next_id,
            started: false,
            last_save: Instant::now(),
            resolver: None,
            ffmpeg: None,
        })
    }

    /// Resolve YouTube streams with this function instead of yt-dlp (tests).
    pub fn set_resolver(&mut self, r: Resolver) {
        self.resolver = Some(r);
    }

    /// Use this ffmpeg (or none) instead of the one on PATH (tests).
    pub fn set_ffmpeg(&mut self, ffmpeg: Option<std::path::PathBuf>) {
        self.ffmpeg = Some(ffmpeg);
    }

    fn save(&mut self, id: i64) {
        if let Some(d) = self.downloads.get(&id) {
            let _ = self.store.save_download(d);
        }
    }

    // ----------------------------------------------------------------- adding

    pub fn client(&self) -> Result<HttpClient, OpenError> {
        HttpClient::new(
            &self.settings.user_agent,
            &self.settings.proxy,
            self.settings.timeout,
        )
    }

    /// A probe of `url` with these credentials; run the future off the UI thread.
    pub fn probe(
        &self,
        url: &str,
        referer: &str,
        username: &str,
        password: &str,
        headers: IndexMap<String, String>,
    ) -> impl std::future::Future<Output = Result<ProbeResult, OpenError>> + Send + 'static {
        let mut tmp = Download::new(0, url, "", "");
        tmp.referer = referer.into();
        tmp.username = username.into();
        tmp.password = password.into();
        tmp.headers = headers;
        let client = self.client();
        let url = url.to_string();
        async move { client?.probe(&url, &request_headers(&tmp), false).await }
    }

    /// The first download with exactly this address.
    pub fn find_duplicate(&self, url: &str) -> Option<&Download> {
        self.downloads.values().find(|d| d.url == url)
    }

    /// A download from the current state of the list (the running task's copy when it runs).
    pub fn get(&self, id: i64) -> Option<Download> {
        match self.tasks.get(&id) {
            Some(r) => Some(r.task.download()),
            None => self.downloads.get(&id).cloned(),
        }
    }

    pub fn add(&mut self, url: &str, filename: &str, o: NewDownload) -> Download {
        let cat = match &o.category {
            Some(name) if !name.is_empty() => self.settings.category(name),
            _ => categories::classify(&self.settings, filename, url),
        }
        .clone();
        let mut d = Download::new(
            self.next_id,
            url,
            filename,
            o.save_dir.as_deref().unwrap_or(&cat.save_dir),
        );
        d.category = cat.name;
        d.description = o.description;
        d.referer = o.referer;
        d.parent_page = o.parent_page;
        d.username = o.username;
        d.password = o.password;
        d.size = o.size.unwrap_or(-1);
        d.resumable = o.resumable;
        d.media = o.media;
        d.headers = o.headers;
        d.checksum = o.checksum;
        self.next_id += 1;
        let id = d.id;
        self.downloads.insert(id, d);
        if !o.queue.is_empty() {
            self.set_queue(&[id], &o.queue);
        }
        self.save(id);
        if o.start {
            self.start(id);
        }
        self.downloads[&id].clone()
    }

    // ----------------------------------------------------------------- control

    pub fn task(&self, id: i64) -> Option<Task> {
        self.tasks.get(&id).map(|r| r.task.clone())
    }

    /// A task exists for the download (it may have just finished and wait for `poll()`).
    pub fn has_task(&self, id: i64) -> bool {
        self.tasks.contains_key(&id)
    }

    /// Downloads with a task.
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Checks reading a file right now.
    pub fn verifier_count(&self) -> usize {
        self.verifiers.len()
    }

    pub fn is_running(&self, id: i64) -> bool {
        self.tasks.get(&id).is_some_and(|r| r.task.is_running())
    }

    fn options(&self, d: &Download) -> EngineOptions {
        let host = url_hostname(&d.url).unwrap_or_default();
        EngineOptions {
            max_connections: if d.connections != 0 {
                d.connections
            } else {
                self.settings.connections_for(&host)
            },
            retries: self.settings.retries,
            retry_delay: self.settings.retry_delay,
            temp_dir: self.settings.temp_dir.clone(),
            request_limit: None,
        }
    }

    /// Start a download unless it is missing, running, complete or being checked.
    pub fn start(&mut self, id: i64) -> Option<Task> {
        let d = self.downloads.get(&id).cloned()?;
        if self.is_running(id) || matches!(d.status, Status::Complete | Status::Verifying) {
            return self.task(id);
        }
        let options = self.options(&d);
        self.serial += 1;
        let serial = self.serial;
        let finished = self.finished.clone();
        let notify = move || {
            finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((id, serial))
        };
        let task = if d.media.is_some() {
            let (ua, proxy, timeout) = (
                self.settings.user_agent.clone(),
                self.settings.proxy.clone(),
                self.settings.timeout,
            );
            let factory: crate::media_task::ClientFactory = Arc::new(move |agent: &str| {
                HttpClient::new(if agent.is_empty() { &ua } else { agent }, &proxy, timeout)
            });
            let mut t = MediaTask::new(
                d,
                factory,
                options,
                Some(self.limiter.clone()),
                self.resolver.clone(),
            );
            if let Some(f) = &self.ffmpeg {
                t = t.with_ffmpeg(f.clone());
            }
            t.on_finish(notify);
            t.start(&self.rt);
            Task::Media(Arc::new(t))
        } else {
            let client = match self.client() {
                Ok(c) => c,
                Err(e) => {
                    // A setting that cannot work (a bad proxy address): fail like a failed connection.
                    let d = self.downloads.get_mut(&id).unwrap();
                    d.status = Status::Error;
                    d.error = e.message();
                    d.last_try = now();
                    let msg = d.error.clone();
                    self.save(id);
                    self.pending
                        .push(Event::download(EventKind::Error, id, msg));
                    return None;
                }
            };
            let t = DownloadTask::new(d, client, options, Some(self.limiter.clone()), None);
            t.on_finish(notify);
            t.start(&self.rt);
            Task::Plain(Arc::new(t))
        };
        self.tasks.insert(
            id,
            Running {
                task: task.clone(),
                serial,
            },
        );
        Some(task)
    }

    pub fn stop(&mut self, id: i64) {
        if let Some(r) = self.tasks.get(&id) {
            r.task.stop();
        }
        self.skip_check(id);
    }

    /// Change a download; a running task gets the user's fields too.
    pub fn edit(&mut self, id: i64, mut f: impl FnMut(&mut Download)) {
        if let Some(d) = self.downloads.get_mut(&id) {
            f(d);
            let copy = d.clone();
            if let Some(r) = self.tasks.get(&id) {
                r.task.update(&mut |t| copy_user_fields(t, &copy));
            }
            let _ = self.store.save_download(&copy);
        }
    }

    /// Save a changed download (1.x `update(d)`); while it runs only the user's fields are taken.
    pub fn update(&mut self, d: &Download) {
        let new = d.clone();
        self.edit(d.id, |x| *x = new.clone());
    }

    pub fn set_download_speed_limit(&mut self, id: i64, kbps: i64) {
        if let Some(d) = self.downloads.get_mut(&id) {
            d.speed_limit_kbps = kbps;
        }
        if let Some(r) = self.tasks.get(&id) {
            r.task.set_speed_limit(kbps);
        }
    }

    // ----------------------------------------------------------------- checksum

    /// Start reading the finished file to compare it with its checksum; false when there is nothing to check.
    pub fn verify(&mut self, id: i64, manual: bool) -> bool {
        let Some(d) = self.downloads.get(&id) else {
            return false;
        };
        if d.checksum.is_empty()
            || self.verifiers.contains_key(&id)
            || !matches!(d.status, Status::Complete | Status::Verifying)
            || !Path::new(&d.path()).is_file()
        {
            return false;
        }
        let v = Arc::new(Verifier::new(d.path(), &d.checksum, manual, id));
        self.downloads.get_mut(&id).unwrap().status = Status::Verifying;
        self.verifiers.insert(id, v.clone());
        let tx = self.verified_tx.clone();
        let _ = std::thread::Builder::new()
            .name(format!("verify-{id}"))
            .spawn(move || {
                v.run();
                let _ = tx.send(id);
            });
        true
    }

    /// "Check hash…" on an unfinished download: keep the checksum and check it when the file is complete.
    pub fn check_when_finished(&mut self, id: i64, checksum: &str) {
        let cs = checksum.to_string();
        self.edit(id, |d| {
            d.checksum = cs.clone();
            d.checksum_force = true;
            d.checksum_result.clear();
            d.checksum_actual.clear();
        });
    }

    /// Keep the outcome of a "Check hash…" run on the download (its "Save this checksum" box).
    pub fn record_check(
        &mut self,
        id: i64,
        checksum: &str,
        result: &str,
        actual: &str,
        seconds: f64,
    ) {
        let (cs, res, act) = (checksum.to_string(), result.to_string(), actual.to_string());
        self.edit(id, |d| {
            d.checksum = cs.clone();
            d.checksum_result = res.clone();
            d.checksum_actual = act.clone();
            d.checksum_checked = now();
            d.checksum_seconds = seconds;
            if res == "verified" {
                d.checksum_retried = false;
            }
        });
    }

    /// "Skip check": the download ends as a plain Complete, unverified.
    pub fn skip_check(&mut self, id: i64) {
        if let Some(v) = self.verifiers.get(&id) {
            v.skip();
        }
    }

    pub fn verify_progress(&self, id: i64) -> Option<f64> {
        self.verifiers.get(&id).map(|v| v.progress())
    }

    fn checked(&mut self, id: i64) -> Vec<Event> {
        let Some(v) = self.verifiers.remove(&id) else {
            return vec![];
        };
        let Some(out) = v.outcome() else {
            return vec![];
        };
        let Some(d) = self.downloads.get_mut(&id) else {
            return vec![];
        };
        if out.result == CheckResult::Error {
            d.status = Status::Error;
            d.error = format!("Checksum check failed: {}", out.error);
            let msg = d.error.clone();
            self.save(id);
            return vec![Event::download(EventKind::Error, id, msg)];
        }
        d.status = Status::Complete;
        d.checksum_result = out.result.as_str().into();
        if out.result != CheckResult::Skipped {
            d.checksum_actual = out.actual.clone();
            d.checksum_checked = now();
            d.checksum_seconds = out.seconds;
        }
        if out.result == CheckResult::Verified {
            d.checksum_retried = false;
        }
        let retried = d.checksum_retried;
        self.save(id);
        if v.manual {
            return vec![Event::download(EventKind::Checked, id, out.result.as_str())];
        }
        if out.result != CheckResult::Mismatch {
            return vec![Event::download(EventKind::Complete, id, "")];
        }
        let action = self.settings.checksum_mismatch.clone();
        if action == "redownload" && !retried {
            self.downloads.get_mut(&id).unwrap().checksum_retried = true; // a second mismatch always asks
            self.redownload(id, true, true);
            return vec![Event::download(
                EventKind::Changed,
                id,
                "checksum mismatch, downloading again",
            )];
        }
        vec![Event::download(
            EventKind::Mismatch,
            id,
            if action == "keep" { "kept" } else { "" },
        )]
    }

    /// Stop every download and every queue; checks keep running.
    pub fn stop_all(&mut self) {
        for r in self.tasks.values() {
            r.task.stop();
        }
        for q in self.queues.values_mut() {
            q.running = false;
        }
    }

    /// Download again from the start (deleting the file when `overwrite`).
    pub fn redownload(&mut self, id: i64, overwrite: bool, start: bool) {
        if !self.downloads.contains_key(&id) || self.is_running(id) {
            return;
        }
        self.skip_check(id);
        let temp = self.settings.temp_dir.clone();
        let d = self.downloads.get_mut(&id).unwrap();
        if overwrite && Path::new(&d.path()).exists() {
            let _ = fs::remove_file(d.path());
        }
        let size = if d.media.is_some() {
            reset_streams(d);
            remove_parts(&temp, id);
            d.media
                .as_ref()
                .and_then(|m| m.get("variant"))
                .and_then(|v| v.get("size"))
                .and_then(Value::as_i64)
                .unwrap_or(-1)
        } else {
            -1
        };
        d.status = Status::Queued;
        d.downloaded = 0;
        d.segments.clear();
        d.size = size;
        d.resumable = None;
        d.error.clear();
        d.checksum_result.clear();
        d.checksum_actual.clear();
        self.save(id);
        if start {
            self.start(id);
        }
    }

    /// Remove downloads (and their part files; the finished files too when `delete_files`).
    pub fn remove(&mut self, ids: &[i64], delete_files: bool) {
        for &id in ids {
            if let Some(r) = self.tasks.remove(&id) {
                r.task.stop();
                r.task.wait(Duration::from_secs(5));
            }
            if let Some(v) = self.verifiers.remove(&id) {
                v.skip();
                let start = Instant::now();
                while v.is_running() && start.elapsed() < Duration::from_secs(5) {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let Some(d) = self.downloads.remove(&id) else {
                continue;
            };
            let part = Path::new(&self.settings.temp_dir).join(format!("{id}.part"));
            if part.exists() {
                let _ = fs::remove_file(part);
            }
            if d.media.is_some() {
                remove_parts(&self.settings.temp_dir, id);
            }
            if delete_files && d.status == Status::Complete && Path::new(&d.path()).exists() {
                let _ = fs::remove_file(d.path());
            }
        }
        let _ = self.store.delete_downloads(ids);
    }

    pub fn remove_completed(&mut self) -> Vec<i64> {
        let ids: Vec<i64> = self
            .downloads
            .values()
            .filter(|d| d.status == Status::Complete)
            .map(|d| d.id)
            .collect();
        self.remove(&ids, false);
        ids
    }

    pub fn set_speed_limiter(&mut self, on: bool, kbps: Option<i64>) {
        if let Some(k) = kbps {
            self.settings.speed_limit_kbps = k;
        }
        self.settings.speed_limiter_on = on;
        self.limiter.set_kbps(if on {
            self.settings.speed_limit_kbps
        } else {
            0
        });
    }

    // ----------------------------------------------------------------- queues

    /// The downloads of a queue in queue order.
    pub fn queue_items(&self, name: &str) -> Vec<&Download> {
        let mut v: Vec<&Download> = self
            .downloads
            .values()
            .filter(|d| d.queue == name)
            .collect();
        v.sort_by_key(|d| d.queue_order);
        v
    }

    fn queue_ids(&self, name: &str) -> Vec<i64> {
        self.queue_items(name).iter().map(|d| d.id).collect()
    }

    /// Put downloads into a queue ("" = none), after the ones already there.
    pub fn set_queue(&mut self, ids: &[i64], name: &str) {
        let mut order = if name.is_empty() {
            0
        } else {
            self.queue_items(name)
                .iter()
                .map(|d| d.queue_order)
                .max()
                .unwrap_or(0)
        };
        for &id in ids {
            if self.downloads.get(&id).is_some_and(|d| d.queue != name) {
                order += 1;
                let (n, o) = (name.to_string(), order);
                self.edit(id, |d| {
                    d.queue = n.clone();
                    d.queue_order = o;
                });
            }
        }
    }

    /// Move a download up (negative) or down in its queue.
    pub fn move_in_queue(&mut self, id: i64, delta: i64) {
        let Some(queue) = self.downloads.get(&id).map(|d| d.queue.clone()) else {
            return;
        };
        let mut items = self.queue_ids(&queue);
        let Some(i) = items.iter().position(|x| *x == id) else {
            return;
        };
        let j = (i as i64 + delta).clamp(0, items.len() as i64 - 1) as usize;
        let moved = items.remove(i);
        items.insert(j, moved);
        for (n, item) in items.iter().enumerate() {
            let order = n as i64 + 1;
            self.edit(*item, |d| d.queue_order = order);
        }
    }

    pub fn add_queue(&mut self, name: &str) {
        self.queues
            .entry(name.to_string())
            .or_insert_with(|| Queue::new(name, QueueSchedule::default()));
        self.save_queue(name);
    }

    /// Delete a queue (not the two built-in ones); its downloads stay, without a queue.
    pub fn delete_queue(&mut self, name: &str) {
        if name == MAIN_QUEUE || name == SYNC_QUEUE || !self.queues.contains_key(name) {
            return;
        }
        self.stop_queue(name);
        for id in self.queue_ids(name) {
            self.edit(id, |d| d.queue.clear());
        }
        self.queues.shift_remove(name);
        let _ = self.store.delete_queue(name);
    }

    pub fn save_queue(&mut self, name: &str) {
        if let Some(q) = self.queues.get(name) {
            let record = QueueRecord {
                schedule: q.schedule.clone(),
                extra: q.extra.clone(),
            };
            let _ = self.store.save_queue(name, &record);
        }
    }

    pub fn start_queue(&mut self, name: &str) -> Vec<Event> {
        let Some(q) = self.queues.get_mut(name) else {
            return vec![];
        };
        if q.running {
            return vec![];
        }
        q.running = true;
        q.failures.clear();
        if q.schedule.periodic {
            for id in self.queue_ids(name) {
                if self.downloads[&id].status == Status::Complete {
                    self.redownload(id, true, false);
                }
            }
        }
        let mut events = vec![Event::queue(EventKind::QueueStarted, name, "")];
        events.extend(self.process_queue(name));
        events
    }

    pub fn stop_queue(&mut self, name: &str) {
        if let Some(q) = self.queues.get_mut(name) {
            q.running = false;
        }
        for id in self.queue_ids(name) {
            self.stop(id);
        }
    }

    fn process_queue(&mut self, name: &str) -> Vec<Event> {
        let Some(q) = self.queues.get(name) else {
            return vec![];
        };
        if !q.running {
            return vec![];
        }
        let limit = if q.schedule.retries_enabled {
            q.schedule.retries
        } else {
            0
        };
        let max_parallel = q.schedule.max_parallel;
        let failures = q.failures.clone();
        let ids = self.queue_ids(name);
        let mut active: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|i| self.is_running(*i))
            .collect();
        let waiting: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|i| {
                let d = &self.downloads[i];
                !self.is_running(*i)
                    && d.status != Status::Complete
                    && (d.status != Status::Error || failures.get(i).copied().unwrap_or(0) <= limit)
            })
            .collect();
        let free = (max_parallel - active.len() as i64).max(0) as usize;
        for id in waiting.into_iter().take(free) {
            self.start(id);
            active.push(id);
        }
        if !active.is_empty() {
            return vec![];
        }
        let q = self.queues.get_mut(name).unwrap();
        q.running = false;
        if q.schedule.periodic {
            q.next_run =
                now() + (q.schedule.every_hours * 3600 + q.schedule.every_minutes * 60) as f64;
        }
        vec![Event::queue(EventKind::QueueDone, name, "")]
    }

    // ----------------------------------------------------------------- scheduler

    fn due(time: &str, now: NaiveDateTime) -> bool {
        now.format("%H:%M").to_string() == time
    }

    fn tick_schedules(&mut self, now_local: NaiveDateTime) -> Vec<Event> {
        let mut events = Vec::new();
        let stamp = now_local.format("%Y-%m-%d %H:%M").to_string();
        let today = now_local.format("%Y-%m-%d").to_string();
        let weekday = now_local.weekday().num_days_from_monday() as i64;
        let names: Vec<String> = self.queues.keys().cloned().collect();
        for name in names {
            let q = &self.queues[&name];
            let s = q.schedule.clone();
            let start_key = format!("start {stamp}");
            if s.start_enabled
                && !q.running
                && Self::due(&s.start_time, now_local)
                && !q.fired.contains(&start_key)
            {
                let day_ok = if s.daily {
                    s.days.contains(&weekday)
                } else {
                    s.once_date.is_empty() || s.once_date == today
                };
                if day_ok {
                    self.queues.get_mut(&name).unwrap().fired.insert(start_key);
                    events.extend(self.start_queue(&name));
                }
            }
            let q = &self.queues[&name];
            let stop_key = format!("stop {stamp}");
            if s.stop_enabled
                && q.running
                && Self::due(&s.stop_time, now_local)
                && !q.fired.contains(&stop_key)
            {
                self.queues.get_mut(&name).unwrap().fired.insert(stop_key);
                self.stop_queue(&name);
                events.push(Event::queue(
                    EventKind::QueueDone,
                    &name,
                    "stopped by schedule",
                ));
            }
            let q = &self.queues[&name];
            if s.periodic && q.next_run != 0.0 && !q.running && now() >= q.next_run {
                self.queues.get_mut(&name).unwrap().next_run = 0.0;
                events.extend(self.start_queue(&name));
            }
        }
        events
    }

    // ----------------------------------------------------------------- poll

    /// Copy the running tasks' downloads into the list.
    fn sync_running(&mut self) {
        for (id, r) in &self.tasks {
            if let Some(slot) = self.downloads.get_mut(id) {
                *slot = r.task.download();
            }
        }
    }

    /// Handle what happened since the last call; `now` is local time (the scheduler's clock).
    pub fn poll(&mut self, now_local: Option<NaiveDateTime>) -> Vec<Event> {
        let mut events = std::mem::take(&mut self.pending);
        if !self.started {
            self.started = true;
            let names: Vec<String> = self
                .queues
                .values()
                .filter(|q| q.schedule.start_on_startup)
                .map(|q| q.name.clone())
                .collect();
            for name in names {
                events.extend(self.start_queue(&name));
            }
        }
        let finished =
            std::mem::take(&mut *self.finished.lock().unwrap_or_else(|e| e.into_inner()));
        for (id, serial) in finished {
            // A newer task of the same download may already run; this report is then stale.
            if self.tasks.get(&id).is_none_or(|r| r.serial != serial) {
                continue;
            }
            let r = self.tasks.remove(&id).unwrap();
            if !self.downloads.contains_key(&id) {
                continue;
            }
            let d = r.task.download();
            self.downloads.insert(id, d.clone());
            let check = self.settings.verify_checksum || d.checksum_force;
            if d.status == Status::Complete
                && !d.checksum.is_empty()
                && check
                && self.verify(id, false)
            {
                self.save(id);
                events.push(Event::download(EventKind::Changed, id, "")); // "complete" follows once the checksum is read
                continue;
            }
            self.save(id);
            match d.status {
                Status::Complete => events.push(Event::download(EventKind::Complete, id, "")),
                Status::Error => {
                    if let Some(q) = self.queues.get_mut(&d.queue)
                        && q.running
                    {
                        *q.failures.entry(id).or_insert(0) += 1;
                    }
                    events.push(Event::download(EventKind::Error, id, d.error.clone()));
                }
                _ => events.push(Event::download(EventKind::Changed, id, "")),
            }
        }
        while let Ok(id) = self.verified_rx.try_recv() {
            events.extend(self.checked(id));
        }
        let names: Vec<String> = self.queues.keys().cloned().collect();
        for name in names {
            events.extend(self.process_queue(&name));
        }
        events.extend(self.tick_schedules(now_local.unwrap_or_else(|| Local::now().naive_local())));
        self.sync_running();
        if self.last_save.elapsed() > Duration::from_secs(5) && !self.tasks.is_empty() {
            let running: Vec<Download> = self.tasks.values().map(|r| r.task.download()).collect();
            let _ = self.store.save_downloads(&running);
            self.last_save = Instant::now();
        }
        events
    }

    /// Stop everything and save the whole list (closing LID).
    pub fn shutdown(&mut self) {
        for v in self.verifiers.values() {
            v.skip();
        }
        for r in self.tasks.values() {
            r.task.stop();
        }
        for r in self.tasks.values() {
            r.task.wait(Duration::from_secs(10));
        }
        self.sync_running();
        let all: Vec<Download> = self.downloads.values().cloned().collect();
        let _ = self.store.save_downloads(&all);
    }
}
