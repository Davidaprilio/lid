//! Multi-connection downloader with dynamic segmentation (`DownloadTask` in `engine.py`).
//!
//! Each connection asks for an open-ended range (`bytes=pos-`) and stops reading when it reaches the end
//! of its segment. When a connection finishes, it takes an abandoned segment or splits the largest
//! remaining one in half and continues there, so all connections stay busy until the file is complete.
//! Bytes are written in place (`pwrite`) into a pre-sized part file in the temporary folder, which is
//! moved to the destination when the download completes.
//!
//! Segments are identified by their `start`, which never changes once a segment exists.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use indexmap::IndexMap;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::http::{HttpClient, OpenError, Response, request_headers};
use crate::limiter::{SpeedLimiter, SpeedMeter};
use crate::models::{Download, Segment, Status, now};
use crate::names::unique_path;

/// Largest read size wanted when no speed limit applies.
pub const CHUNK: i64 = 1024 * 1024;
/// Smaller reads keep a speed limit smooth.
pub const LIMITED_CHUNK: i64 = 64 * 1024;
/// Never create a segment smaller than this.
pub const MIN_SPLIT: i64 = 256 * 1024;
/// Answers that fail a connection at once, without retrying.
pub const FATAL_HTTP: [u16; 7] = [400, 401, 403, 404, 405, 410, 451];

const NO_RANGE: &str = "server ignored the Range header";

#[derive(Debug, Clone, PartialEq)]
pub struct EngineOptions {
    pub max_connections: i64,
    pub retries: i64,
    pub retry_delay: f64,
    pub temp_dir: String,
    /// Ask for at most this many bytes per request (`bytes=a-b`), then go on with the next piece on the
    /// same connection. YouTube slows a long request down to about the playing speed; yt-dlp reports
    /// the piece size it uses (`downloader_options.http_chunk_size`, 10 MB). None: one open-ended
    /// request per connection.
    pub request_limit: Option<i64>,
}

impl Default for EngineOptions {
    fn default() -> Self {
        EngineOptions {
            max_connections: 8,
            retries: 10,
            retry_delay: 3.0,
            temp_dir: "/tmp".into(),
            request_limit: None,
        }
    }
}

/// One connection as the progress dialog shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionInfo {
    /// 1-based; the list never shrinks.
    pub index: usize,
    pub received: i64,
    /// "Connecting...", "Receiving data...", "Reconnecting (n/m)...", "Finished", "Error: …", "HTTP 404 Not Found"
    pub status: String,
    pub alive: bool,
    /// Bytes per second, 0 when the connection has ended.
    pub rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub segments: Vec<Segment>,
    pub connections: Vec<ConnectionInfo>,
    /// Bytes per second over the last seconds.
    pub rate: f64,
    /// Seconds, -1 when unknown.
    pub time_left: i64,
}

struct Conn {
    index: usize,
    /// `start` of the segment this connection works on.
    segment: Option<i64>,
    status: String,
    received: i64,
    alive: bool,
    meter: Arc<SpeedMeter>,
}

struct State {
    d: Download,
    conns: Vec<Conn>,
    error: String,
    splittable: bool,
}

impl State {
    fn seg(&mut self, start: i64) -> &mut Segment {
        self.d
            .segments
            .iter_mut()
            .find(|s| s.start == start)
            .expect("segments are never removed while running")
    }

    fn owned(&self) -> HashSet<i64> {
        self.conns
            .iter()
            .filter(|c| c.alive)
            .filter_map(|c| c.segment)
            .collect()
    }

    /// Halve the segment with the most bytes left (the first one on a tie). Its connection notices the
    /// new end on its next read.
    fn split_largest(&mut self) -> Option<i64> {
        let owned = self.owned();
        let mut best: Option<usize> = None;
        for (i, s) in self.d.segments.iter().enumerate() {
            if s.done() || !owned.contains(&s.start) {
                continue;
            }
            if best.is_none_or(|b| s.remaining() > self.d.segments[b].remaining()) {
                best = Some(i);
            }
        }
        let best = &mut self.d.segments[best?];
        if best.remaining() < 2 * MIN_SPLIT {
            return None;
        }
        let mid = best.pos + best.remaining() / 2;
        let new = Segment::new(mid, best.end, mid);
        best.end = mid;
        self.d.segments.push(new);
        self.d.segments.sort_by_key(|s| s.start);
        Some(mid)
    }

    /// An abandoned segment first, else a split.
    fn next_work(&mut self) -> Option<i64> {
        let owned = self.owned();
        if let Some(s) = self
            .d
            .segments
            .iter()
            .find(|s| !s.done() && !owned.contains(&s.start))
        {
            return Some(s.start);
        }
        if self.splittable {
            self.split_largest()
        } else {
            None
        }
    }

    fn add_conn(&mut self, segment: i64) -> usize {
        let index = self.conns.len() + 1;
        self.conns.push(Conn {
            index,
            segment: Some(segment),
            status: "Connecting...".into(),
            received: 0,
            alive: true,
            meter: Arc::new(SpeedMeter::default()),
        });
        index - 1
    }
}

struct Shared {
    state: Mutex<State>,
    client: HttpClient,
    options: EngineOptions,
    global: Option<Arc<SpeedLimiter>>,
    file_limiter: SpeedLimiter,
    meter: SpeedMeter,
    stop: CancellationToken,
    part_path: PathBuf,
    running: AtomicBool,
    on_finish: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[derive(Debug, PartialEq, Eq)]
enum Fetch {
    Done,
    Stopped,
    Failed,
}

enum FetchError {
    Open(OpenError),
    NoRange,
    Io(String),
}

/// Runs one download on the tokio runtime; the UI reads `snapshot()` and `download()`.
pub struct DownloadTask {
    shared: Arc<Shared>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl DownloadTask {
    /// `part_name` names the part file (`<part_name>.part`); the download id when `None`.
    pub fn new(
        download: Download,
        client: HttpClient,
        options: EngineOptions,
        global: Option<Arc<SpeedLimiter>>,
        part_name: Option<String>,
    ) -> DownloadTask {
        let part_name = part_name.unwrap_or_else(|| download.id.to_string());
        let part_path = Path::new(&options.temp_dir).join(format!("{part_name}.part"));
        let file_limiter = SpeedLimiter::new(download.speed_limit_kbps);
        DownloadTask {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    d: download,
                    conns: Vec::new(),
                    error: String::new(),
                    splittable: false,
                }),
                client,
                options,
                global,
                file_limiter,
                meter: SpeedMeter::default(),
                stop: CancellationToken::new(),
                part_path,
                running: AtomicBool::new(false),
                on_finish: Mutex::new(None),
            }),
            handle: Mutex::new(None),
        }
    }

    /// Called once on the runtime when the task ends, whatever the result.
    pub fn on_finish(&self, f: impl FnOnce() + Send + 'static) {
        *self
            .shared
            .on_finish
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
    }

    pub fn part_path(&self) -> &Path {
        &self.shared.part_path
    }

    pub fn start(&self, rt: &Handle) {
        self.shared.running.store(true, Ordering::Release);
        let shared = self.shared.clone();
        let h = rt.spawn(async move { shared.run().await });
        *self.handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
    }

    /// Ask every connection to stop; the task ends Paused.
    pub fn stop(&self) {
        self.shared.stop.cancel();
    }

    pub fn is_stopping(&self) -> bool {
        self.shared.stop.is_cancelled()
    }

    /// Wait until the task has ended (or `timeout` passed). True when it has ended.
    pub async fn join(&self, timeout: Duration) -> bool {
        let h = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        match h {
            None => !self.is_running(),
            // On a timeout the handle is dropped, which detaches the task; it keeps running.
            Some(h) => tokio::time::timeout(timeout, h).await.is_ok(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    pub fn set_speed_limit(&self, kbps: i64) {
        self.shared.lock().d.speed_limit_kbps = kbps;
        self.shared.file_limiter.set_kbps(kbps);
    }

    /// The last connection or task error ("" when none).
    pub fn error(&self) -> String {
        self.shared.lock().error.clone()
    }

    /// A copy of the download as the task has it now.
    pub fn download(&self) -> Download {
        self.shared.lock().d.clone()
    }

    /// Change the download while it runs (for example its description).
    pub fn update_download(&self, f: impl FnOnce(&mut Download)) {
        f(&mut self.shared.lock().d)
    }

    pub fn snapshot(&self) -> Snapshot {
        let (segments, connections, size, downloaded) = {
            let st = self.shared.lock();
            let conns = st
                .conns
                .iter()
                .map(|c| ConnectionInfo {
                    index: c.index,
                    received: c.received,
                    status: c.status.clone(),
                    alive: c.alive,
                    rate: if c.alive { c.meter.rate() } else { 0.0 },
                })
                .collect();
            (st.d.segments.clone(), conns, st.d.size, st.d.downloaded)
        };
        let rate = self.shared.meter.rate();
        let time_left = if rate > 0.0 && size > 0 {
            ((size - downloaded) as f64 / rate) as i64
        } else {
            -1
        };
        Snapshot {
            segments,
            connections,
            rate,
            time_left,
        }
    }
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        // Another file system: copy, then remove (what shutil.move does).
        Err(e) if e.raw_os_error() == Some(18) => {
            fs::copy(from, to)?;
            fs::remove_file(from)
        }
        Err(e) => Err(e),
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_status(&self, status: Status) {
        self.lock().d.status = status;
    }

    fn conn_status(&self, conn: usize, status: impl Into<String>) {
        self.lock().conns[conn].status = status.into();
    }

    async fn run(self: Arc<Self>) {
        let (url, headers) = {
            let mut st = self.lock();
            st.d.last_try = now();
            st.d.error.clear();
            st.d.status = Status::Connecting;
            (st.d.url.clone(), request_headers(&st.d))
        };
        if let Err(e) = self.clone().prepare_and_download(&url, &headers).await {
            self.lock().error = e;
        }
        self.finish();
        let callback = self
            .on_finish
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.running.store(false, Ordering::Release);
        if let Some(f) = callback {
            f();
        }
    }

    async fn prepare_and_download(
        self: Arc<Self>,
        url: &str,
        headers: &IndexMap<String, String>,
    ) -> Result<(), String> {
        let probe = self
            .client
            .probe(url, headers, true)
            .await
            .map_err(|e| e.message())?;
        let mut first = None;
        {
            let mut st = self.lock();
            let d = &mut st.d;
            let can_resume = !d.segments.is_empty()
                && d.resumable == Some(true)
                && probe.resumable
                && probe.size == d.size
                && self.part_path.exists();
            if !can_resume {
                d.size = probe.size;
                d.resumable = Some(probe.resumable);
                d.segments = vec![Segment::new(
                    0,
                    if probe.size >= 0 { probe.size } else { -1 },
                    0,
                )];
                first = probe.response;
                if self.part_path.exists() {
                    fs::remove_file(&self.part_path).map_err(|e| e.to_string())?;
                }
            }
            d.downloaded = d.segments.iter().map(|s| s.pos - s.start).sum();
            st.splittable = st.d.resumable == Some(true) && st.d.size > 0;
        }

        fs::create_dir_all(&self.options.temp_dir).map_err(|e| e.to_string())?;
        let file = crate::fsio::part_file_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.part_path)
            .map_err(|e| e.to_string())?;
        let size = self.lock().d.size;
        if size >= 0 {
            file.set_len(size as u64).map_err(|e| e.to_string())?;
        }
        if self.stop.is_cancelled() {
            return Ok(());
        }
        self.set_status(Status::Downloading);
        self.spawn_workers(first, Arc::new(file)).await;
        Ok(())
    }

    async fn spawn_workers(self: Arc<Self>, first: Option<Response>, file: Arc<File>) {
        let mut first = first;
        let new: Vec<usize> = {
            let mut st = self.lock();
            let limit = if st.splittable {
                st.options_limit(&self.options)
            } else {
                1
            };
            let pending: Vec<i64> =
                st.d.segments
                    .iter()
                    .filter(|s| !s.done())
                    .map(|s| s.start)
                    .collect();
            if pending.is_empty() {
                return;
            }
            let mut new: Vec<usize> = pending
                .into_iter()
                .take(limit)
                .map(|start| st.add_conn(start))
                .collect();
            while st.splittable && st.conns.len() < limit {
                match st.split_largest() {
                    Some(start) => new.push(st.add_conn(start)),
                    None => break,
                }
            }
            new
        };
        let handles: Vec<JoinHandle<()>> = new
            .into_iter()
            .enumerate()
            .map(|(i, conn)| {
                let response = if i == 0 { first.take() } else { None };
                tokio::spawn(self.clone().worker(conn, response, file.clone()))
            })
            .collect();
        drop(first);
        for h in handles {
            let _ = h.await;
        }
    }

    async fn worker(self: Arc<Self>, conn: usize, response: Option<Response>, file: Arc<File>) {
        let mut response = response;
        loop {
            if self.stop.is_cancelled() {
                break;
            }
            let Some(segment) = self.lock().conns[conn].segment else {
                break;
            };
            let result = self.fetch(conn, segment, response.take(), &file).await;
            let mut st = self.lock();
            if result != Fetch::Done || self.stop.is_cancelled() {
                st.conns[conn].segment = None;
                break;
            }
            let next = st.next_work();
            st.conns[conn].segment = next;
        }
        let mut st = self.lock();
        let c = &mut st.conns[conn];
        if !self.stop.is_cancelled()
            && (c.status.starts_with("Receiving") || c.status.starts_with("Connecting"))
        {
            c.status = "Finished".into();
        }
        c.alive = false;
    }

    async fn fetch(
        &self,
        conn: usize,
        segment: i64,
        response: Option<Response>,
        file: &File,
    ) -> Fetch {
        let mut response = response;
        let mut attempts = 0;
        let retries = self.options.retries;
        let headers = request_headers(&self.lock().d);
        while !self.stop.is_cancelled() {
            let err = match self
                .fetch_once(conn, segment, response.take(), &headers, file)
                .await
            {
                Ok(done) => return done,
                Err(FetchError::NoRange) => {
                    let mut st = self.lock();
                    st.conns[conn].status = format!("Error: {NO_RANGE}");
                    st.error = NO_RANGE.into();
                    return Fetch::Failed;
                }
                Err(FetchError::Open(e @ OpenError::Status { code, .. }))
                    if FATAL_HTTP.contains(&code) =>
                {
                    let mut st = self.lock();
                    st.conns[conn].status = e.message();
                    st.error = e.message();
                    return Fetch::Failed;
                }
                Err(FetchError::Open(e @ OpenError::Invalid(_))) => {
                    let mut st = self.lock();
                    st.conns[conn].status = format!("Error: {e}");
                    st.error = e.message();
                    return Fetch::Failed;
                }
                Err(FetchError::Open(OpenError::Status { code, .. })) => format!("HTTP {code}"),
                Err(FetchError::Open(e)) => e.message(),
                Err(FetchError::Io(m)) => m,
            };
            attempts += 1;
            if attempts > retries {
                let mut st = self.lock();
                st.conns[conn].status = format!("Error: {err}");
                st.error = err;
                return Fetch::Failed;
            }
            {
                let mut st = self.lock();
                if st.d.resumable != Some(true) {
                    // The server cannot continue a partial transfer: start over.
                    let seg = st.seg(segment);
                    let back = seg.pos - seg.start;
                    seg.pos = seg.start;
                    st.d.downloaded -= back;
                    st.conns[conn].received = 0;
                }
                st.conns[conn].status = format!("Reconnecting ({attempts}/{retries})...");
            }
            let delay = Duration::from_secs_f64(self.options.retry_delay.max(0.0));
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.stop.cancelled() => {}
            }
        }
        Fetch::Stopped
    }

    async fn fetch_once(
        &self,
        conn: usize,
        segment: i64,
        response: Option<Response>,
        headers: &IndexMap<String, String>,
        file: &File,
    ) -> Result<Fetch, FetchError> {
        // With a request limit, a response opened before (open-ended) is not used: every request asks
        // for one piece.
        let limit = self.options.request_limit.filter(|n| *n > 0);
        let mut response = if limit.is_some() { None } else { response };
        // The last byte (exclusive) the current request asks for, when it is a piece.
        let mut piece_end: Option<i64> = None;
        let mut resp = match response.take() {
            Some(r) => r,
            None => {
                self.open_piece(conn, segment, headers, limit, &mut piece_end)
                    .await?
            }
        };
        self.conn_status(conn, "Receiving data...");
        let conn_meter = self.lock().conns[conn].meter.clone();
        loop {
            if self.stop.is_cancelled() {
                return Ok(Fetch::Stopped);
            }
            let limited =
                self.file_limiter.rate() > 0 || self.global.as_ref().is_some_and(|g| g.rate() > 0);
            let step = if limited { LIMITED_CHUNK } else { CHUNK };
            let want = {
                let mut st = self.lock();
                let s = st.seg(segment);
                if s.end >= 0 { s.end - s.pos } else { step }
            };
            if want <= 0 {
                return Ok(Fetch::Done);
            }
            let data = tokio::select! {
                d = resp.body.chunk(self.client.timeout) => d.map_err(FetchError::Open)?,
                _ = self.stop.cancelled() => return Ok(Fetch::Stopped),
            };
            let Some(data) = data else {
                // A piece ended where it should: ask for the next one on this connection.
                if let Some(end) = piece_end {
                    let more = {
                        let mut st = self.lock();
                        let s = st.seg(segment);
                        s.pos >= end && (s.end < 0 || s.pos < s.end)
                    };
                    if more {
                        resp = self
                            .open_piece(conn, segment, headers, limit, &mut piece_end)
                            .await?;
                        self.conn_status(conn, "Receiving data...");
                        continue;
                    }
                }
                let mut st = self.lock();
                if st.seg(segment).end < 0 {
                    // Unknown size: the end of the stream is the end of the file.
                    let s = st.seg(segment);
                    s.end = s.pos;
                    st.d.size = st.seg(segment).pos;
                    return Ok(Fetch::Done);
                }
                return Err(FetchError::Io("connection closed by the server".into()));
            };
            if let Some(g) = &self.global {
                g.consume(data.len(), &self.stop).await;
            }
            self.file_limiter.consume(data.len(), &self.stop).await;
            // A split may have moved the end while we waited: count only what is still ours.
            let (offset, allowed) = {
                let mut st = self.lock();
                let s = st.seg(segment);
                let allowed = if s.end < 0 {
                    data.len() as i64
                } else {
                    (data.len() as i64).min(s.end - s.pos).max(0)
                };
                let offset = s.pos;
                s.pos += allowed;
                st.d.downloaded += allowed;
                st.conns[conn].received += allowed;
                (offset, allowed)
            };
            if allowed > 0 {
                if let Err(e) =
                    crate::fsio::write_all_at(file, &data[..allowed as usize], offset as u64)
                {
                    // Give the bytes back so the retry writes them again.
                    let mut st = self.lock();
                    let s = st.seg(segment);
                    if s.pos == offset + allowed {
                        s.pos = offset;
                        st.d.downloaded -= allowed;
                        st.conns[conn].received -= allowed;
                    }
                    return Err(FetchError::Io(e.to_string()));
                }
                self.meter.add(allowed as u64);
                conn_meter.add(allowed as u64);
            }
        }
    }

    /// Open the next request of a segment at its position: open-ended, or one piece of at most `limit`
    /// bytes (never past the segment's end), whose exclusive end goes to `piece_end`.
    async fn open_piece(
        &self,
        conn: usize,
        segment: i64,
        headers: &IndexMap<String, String>,
        limit: Option<i64>,
        piece_end: &mut Option<i64>,
    ) -> Result<Response, FetchError> {
        self.conn_status(conn, "Connecting...");
        let (url, pos, ranged, seg_end) = {
            let mut st = self.lock();
            let splittable = st.splittable;
            let s = st.seg(segment);
            let (pos, end) = (s.pos, s.end);
            (
                st.d.url.clone(),
                pos,
                splittable || pos > 0 || limit.is_some(),
                end,
            )
        };
        let end = limit.map(|n| {
            if seg_end >= 0 {
                (pos + n).min(seg_end)
            } else {
                pos + n
            }
        });
        *piece_end = end;
        let r = self
            .client
            .open_span(&url, pos, end.map(|e| e - 1), ranged, headers)
            .await
            .map_err(FetchError::Open)?;
        if ranged && r.status != 206 {
            return Err(FetchError::NoRange);
        }
        Ok(r)
    }

    fn finish(&self) {
        let stopped = self.stop.is_cancelled();
        let (complete, save_dir, path) = {
            let st = self.lock();
            let d = &st.d;
            (
                !d.segments.is_empty() && d.segments.iter().all(|s| s.done()),
                d.save_dir.clone(),
                d.path(),
            )
        };
        if complete && !stopped {
            self.set_status(Status::Assembling);
            let target = fs::create_dir_all(&save_dir).map(|()| unique_path(&path));
            let moved = target.and_then(|t| move_file(&self.part_path, Path::new(&t)).map(|()| t));
            let mut st = self.lock();
            match moved {
                Ok(target) => {
                    st.d.filename = Path::new(&target)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    st.d.size = st.d.downloaded;
                    st.d.segments.clear();
                    st.d.status = Status::Complete;
                }
                Err(e) => {
                    let msg = format!("Cannot save the file: {e}");
                    st.error = msg.clone();
                    st.d.error = msg;
                    st.d.status = Status::Error;
                }
            }
        } else if stopped {
            self.set_status(Status::Paused);
        } else {
            let mut st = self.lock();
            st.d.error = if st.error.is_empty() {
                "Download failed".into()
            } else {
                st.error.clone()
            };
            st.d.status = Status::Error;
        }
    }
}

impl State {
    fn options_limit(&self, options: &EngineOptions) -> usize {
        options.max_connections.max(1) as usize
    }
}
