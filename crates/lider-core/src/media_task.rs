//! A YouTube download (`media_task.py` in 1.x): resolve fresh stream URLs, fetch each stream with the
//! normal engine, then join or convert them with ffmpeg. Stream URLs expire, so they are resolved again on
//! every start; the per-stream progress lives in `Download::media["streams"]`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::engine::{DownloadTask, EngineOptions, Snapshot};
use crate::http::{HttpClient, OpenError};
use crate::limiter::SpeedLimiter;
use crate::media::{self, Resolved, Variant};
use crate::models::{Download, Segment, Status, now};
use crate::names::unique_path;

/// Resolves format ids of a page to fresh stream URLs (blocking; runs yt-dlp).
pub type Resolver = Arc<dyn Fn(&str, &[String]) -> Result<Vec<Resolved>, String> + Send + Sync>;
/// Builds the HTTP client for a stream from the user agent YouTube asks for ("" = the default one).
pub type ClientFactory = Arc<dyn Fn(&str) -> Result<HttpClient, OpenError> + Send + Sync>;

const STREAM_KEYS: [&str; 6] = [
    "segments",
    "size",
    "resumable",
    "done",
    "file",
    "downloaded",
];
const PROGRESS_KEYS: [&str; 4] = ["segments", "done", "file", "downloaded"];

pub fn part_prefix(id: i64) -> String {
    format!("{id}-media")
}

/// Delete every part file of a media download (`<temp>/<id>-media*`).
pub fn remove_parts(temp_dir: &str, id: i64) {
    let prefix = part_prefix(id);
    if let Ok(entries) = fs::read_dir(temp_dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

/// Forget the progress of every stream (Download again).
pub fn reset_streams(d: &mut Download) {
    for s in streams_mut(d) {
        for k in STREAM_KEYS {
            s.remove(k);
        }
    }
}

fn streams_mut(d: &mut Download) -> impl Iterator<Item = &mut Map<String, Value>> {
    d.media
        .as_mut()
        .and_then(|m| m.get_mut("streams"))
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
}

fn streams(d: &Download) -> Vec<Map<String, Value>> {
    d.media
        .as_ref()
        .and_then(|m| m.get("streams"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|s| s.as_object().cloned()).collect())
        .unwrap_or_default()
}

fn stream_mut(d: &mut Download, i: usize) -> &mut Map<String, Value> {
    streams_mut(d).nth(i).expect("stream index exists")
}

fn int(s: &Map<String, Value>, key: &str) -> i64 {
    s.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn flag(s: &Map<String, Value>, key: &str) -> bool {
    s.get(key).is_some_and(crate::pyutil::truthy)
}

fn string(s: &Map<String, Value>, key: &str) -> String {
    s.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn title_case(kind: &str) -> String {
    let mut c = kind.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

struct Shared {
    d: Mutex<Download>,
    inner: Mutex<Option<Arc<DownloadTask>>>,
    current: Mutex<Option<usize>>,
    status_text: Mutex<String>,
    error: Mutex<String>,
    stop: CancellationToken,
    running: AtomicBool,
    on_finish: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    make_client: ClientFactory,
    options: EngineOptions,
    global: Option<Arc<SpeedLimiter>>,
    resolver: Resolver,
    ffmpeg: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct MediaTask {
    shared: Arc<Shared>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl MediaTask {
    pub fn new(
        download: Download,
        make_client: ClientFactory,
        options: EngineOptions,
        global: Option<Arc<SpeedLimiter>>,
        resolver: Option<Resolver>,
    ) -> MediaTask {
        let resolver = resolver.unwrap_or_else(|| {
            Arc::new(|page: &str, ids: &[String]| media::resolve_streams(page, ids))
        });
        MediaTask {
            shared: Arc::new(Shared {
                d: Mutex::new(download),
                inner: Mutex::new(None),
                current: Mutex::new(None),
                status_text: Mutex::new(String::new()),
                error: Mutex::new(String::new()),
                stop: CancellationToken::new(),
                running: AtomicBool::new(false),
                on_finish: Mutex::new(None),
                make_client,
                options,
                global,
                resolver,
                ffmpeg: media::ffmpeg().map(Path::to_path_buf),
            }),
            handle: Mutex::new(None),
        }
    }

    /// Use this ffmpeg (or none) instead of the one on PATH.
    pub fn with_ffmpeg(self, ffmpeg: Option<PathBuf>) -> MediaTask {
        let shared = Arc::try_unwrap(self.shared).ok().expect("not started yet");
        MediaTask {
            shared: Arc::new(Shared { ffmpeg, ..shared }),
            handle: self.handle,
        }
    }

    pub fn on_finish(&self, f: impl FnOnce() + Send + 'static) {
        *lock(&self.shared.on_finish) = Some(Box::new(f));
    }

    pub fn start(&self, rt: &Handle) {
        self.shared.running.store(true, Ordering::Release);
        let s = self.shared.clone();
        *lock(&self.handle) = Some(rt.spawn(async move { s.run().await }));
    }

    pub fn stop(&self) {
        self.shared.stop.cancel();
        if let Some(t) = lock(&self.shared.inner).as_ref() {
            t.stop();
        }
    }

    pub fn is_stopping(&self) -> bool {
        self.shared.stop.is_cancelled()
    }

    pub async fn join(&self, timeout: Duration) -> bool {
        let h = lock(&self.handle).take();
        match h {
            None => !self.is_running(),
            Some(h) => tokio::time::timeout(timeout, h).await.is_ok(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    pub fn set_speed_limit(&self, kbps: i64) {
        lock(&self.shared.d).speed_limit_kbps = kbps;
        if let Some(t) = lock(&self.shared.inner).as_ref() {
            t.set_speed_limit(kbps);
        }
    }

    pub fn error(&self) -> String {
        lock(&self.shared.error).clone()
    }

    /// The phase shown in the status column ("Getting the video address...", "Receiving video...", …).
    pub fn status_text(&self) -> String {
        lock(&self.shared.status_text).clone()
    }

    pub fn download(&self) -> Download {
        lock(&self.shared.d).clone()
    }

    pub fn update_download(&self, f: impl FnOnce(&mut Download)) {
        f(&mut lock(&self.shared.d))
    }

    /// The streams laid end to end: their segments, the current stream's connections ("Video: …").
    pub fn snapshot(&self) -> Snapshot {
        let d = self.download();
        let inner = lock(&self.shared.inner).clone();
        let current = *lock(&self.shared.current);
        let inner_snap = inner.map(|t| t.snapshot());
        let mut segments = Vec::new();
        let mut connections = Vec::new();
        let mut offset = 0;
        for (i, s) in streams(&d).iter().enumerate() {
            let size = int(s, "size").max(0);
            let parts: Vec<Segment> = match (&inner_snap, current) {
                (Some(snap), Some(c)) if c == i => {
                    let kind = title_case(&string(s, "kind"));
                    connections = snap
                        .connections
                        .iter()
                        .map(|c| crate::engine::ConnectionInfo {
                            status: format!("{kind}: {}", c.status),
                            ..c.clone()
                        })
                        .collect();
                    snap.segments.clone()
                }
                _ if flag(s, "done") => vec![Segment::new(0, size, size)],
                _ => segment_list(s),
            };
            segments.extend(parts.into_iter().map(|p| {
                Segment::new(
                    p.start + offset,
                    if p.end >= 0 { p.end + offset } else { -1 },
                    p.pos + offset,
                )
            }));
            offset += size;
        }
        let rate = inner_snap.as_ref().map_or(0.0, |s| s.rate);
        let time_left = if rate > 0.0 && d.size > 0 {
            ((d.size - d.downloaded) as f64 / rate) as i64
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

/// A stream's saved segments, stored as `[start, end, pos]` lists.
fn segment_list(s: &Map<String, Value>) -> Vec<Segment> {
    s.get("segments")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let v = x.as_array()?;
                    Some(Segment::new(
                        v.first()?.as_i64()?,
                        v.get(1)?.as_i64()?,
                        v.get(2)?.as_i64()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Shared {
    fn d(&self) -> MutexGuard<'_, Download> {
        lock(&self.d)
    }

    fn set_text(&self, t: &str) {
        *lock(&self.status_text) = t.to_string();
    }

    fn done_bytes(&self) -> i64 {
        streams(&self.d())
            .iter()
            .filter(|s| flag(s, "done"))
            .map(|s| int(s, "size").max(0))
            .sum()
    }

    fn update_totals(&self) {
        let mut d = self.d();
        let sizes: Vec<i64> = streams(&d)
            .iter()
            .map(|s| match int(s, "size") {
                0 => -1,
                n => n,
            })
            .collect();
        if sizes.iter().all(|n| *n > 0) {
            d.size = sizes.iter().sum();
        }
    }

    async fn resolve(&self, page: String, ids: Vec<String>) -> Result<Vec<Resolved>, String> {
        let r = self.resolver.clone();
        tokio::task::spawn_blocking(move || r(&page, &ids))
            .await
            .map_err(|e| e.to_string())?
    }

    async fn run(self: Arc<Self>) {
        {
            let mut d = self.d();
            d.last_try = now();
            d.error.clear();
            d.status = Status::Connecting;
        }
        self.set_text("Getting the video address...");
        if let Err(e) = self.work().await {
            let msg = media::clean_error(&e);
            let msg = if msg.is_empty() { "Error".into() } else { msg };
            *lock(&self.error) = msg.clone();
            let mut d = self.d();
            d.error = msg;
            d.status = Status::Error;
        }
        *lock(&self.inner) = None;
        *lock(&self.current) = None;
        self.set_text("");
        let callback = lock(&self.on_finish).take();
        self.running.store(false, Ordering::Release);
        if let Some(f) = callback {
            f();
        }
    }

    fn page_url(&self) -> String {
        self.d()
            .media
            .as_ref()
            .and_then(|m| m.get("page_url"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    async fn work(&self) -> Result<(), String> {
        let ids: Vec<String> = streams(&self.d())
            .iter()
            .map(|s| string(s, "format_id"))
            .collect();
        let fresh = self.resolve(self.page_url(), ids).await?;
        {
            let mut d = self.d();
            for (s, f) in streams_mut(&mut d).zip(&fresh) {
                let old = int(s, "size");
                if old > 0 && f.size > 0 && f.size != old {
                    // The stream changed on YouTube's side: start it again.
                    for k in PROGRESS_KEYS {
                        s.remove(k);
                    }
                }
                s.insert("url".into(), f.url.clone().into());
                s.insert("headers".into(), Value::Object(f.headers.clone()));
                s.insert("ext".into(), f.ext.clone().into());
                s.insert("chunk".into(), f.chunk.into());
                if f.size > 0 && int(s, "size") == 0 {
                    s.insert("size".into(), f.size.into());
                }
            }
        }
        self.update_totals();
        let count = streams(&self.d()).len();
        for i in 0..count {
            if self.stop.is_cancelled() {
                break;
            }
            {
                let mut d = self.d();
                let s = stream_mut(&mut d, i);
                if flag(s, "done") {
                    if Path::new(&string(s, "file")).exists() {
                        continue;
                    }
                    for k in PROGRESS_KEYS {
                        s.remove(k); // the finished part was deleted: fetch it again
                    }
                }
            }
            *lock(&self.current) = Some(i);
            let mut ok = self.download_stream(i).await?;
            let mut refreshes = 0;
            // YouTube sometimes refuses a stream URL (403); a freshly resolved one usually works.
            while !ok
                && !self.stop.is_cancelled()
                && self.d().error.contains("HTTP 403")
                && refreshes < 2
            {
                refreshes += 1;
                self.set_text("Getting a new video address...");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    _ = self.stop.cancelled() => {}
                }
                let fid = string(&streams(&self.d())[i], "format_id");
                let f = self
                    .resolve(self.page_url(), vec![fid])
                    .await?
                    .into_iter()
                    .next()
                    .ok_or("yt-dlp returned no stream")?;
                {
                    let mut d = self.d();
                    let s = stream_mut(&mut d, i);
                    s.insert("url".into(), f.url.into());
                    s.insert("headers".into(), Value::Object(f.headers));
                    d.error.clear();
                }
                lock(&self.error).clear();
                ok = self.download_stream(i).await?;
            }
            if !ok {
                break;
            }
        }
        let all_done = streams(&self.d()).iter().all(|s| flag(s, "done"));
        if self.stop.is_cancelled() {
            self.d().status = Status::Paused;
        } else if all_done {
            self.assemble().await?;
        } else {
            let mut d = self.d();
            if d.status != Status::Error {
                d.status = Status::Error;
            }
        }
        Ok(())
    }

    async fn download_stream(&self, i: usize) -> Result<bool, String> {
        let (sub, kind, name) = {
            let d = self.d();
            let s = &streams(&d)[i];
            let kind = string(s, "kind");
            let name = format!("{}-{kind}", part_prefix(d.id));
            let mut sub = Download::new(
                d.id,
                &string(s, "url"),
                &format!("{name}.{}", string(s, "ext")),
                &self.options.temp_dir,
            );
            sub.size = match int(s, "size") {
                0 => -1,
                n => n,
            };
            sub.resumable = s.get("resumable").and_then(Value::as_bool);
            sub.segments = segment_list(s);
            sub.speed_limit_kbps = d.speed_limit_kbps;
            if let Some(Value::Object(h)) = s.get("headers") {
                sub.headers = h
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect();
            }
            (sub, kind, name)
        };
        if sub.segments.is_empty() {
            let stale = Path::new(&self.options.temp_dir).join(&sub.filename);
            if stale.exists() {
                fs::remove_file(&stale).map_err(|e| e.to_string())?;
            }
        }
        let ua = sub.headers.get("User-Agent").cloned().unwrap_or_default();
        let client = (self.make_client)(&ua).map_err(|e| e.message())?;
        // Pieces of the size yt-dlp says the site serves at full speed.
        let mut options = self.options.clone();
        let chunk = int(&streams(&self.d())[i], "chunk");
        options.request_limit = (chunk > 0).then_some(chunk);
        let task = Arc::new(DownloadTask::new(
            sub,
            client,
            options,
            self.global.clone(),
            Some(name),
        ));
        *lock(&self.inner) = Some(task.clone());
        self.d().status = Status::Downloading;
        self.set_text(&format!("Receiving {kind}..."));
        let done_before = self.done_bytes();
        if self.stop.is_cancelled() {
            task.stop();
        }
        task.start(&Handle::current());
        while task.is_running() {
            let sd = task.download();
            let size_changed = {
                let mut d = self.d();
                d.downloaded = done_before + sd.downloaded;
                let s = stream_mut(&mut d, i);
                if sd.size > 0 && int(s, "size") != sd.size {
                    s.insert("size".into(), sd.size.into());
                    true
                } else {
                    false
                }
            };
            if size_changed {
                self.update_totals();
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        task.join(Duration::from_secs(10)).await;
        let sd = task.download();
        {
            let mut d = self.d();
            let s = stream_mut(&mut d, i);
            s.insert(
                "segments".into(),
                Value::Array(
                    sd.segments
                        .iter()
                        .map(|x| json!([x.start, x.end, x.pos]))
                        .collect(),
                ),
            );
            s.insert(
                "resumable".into(),
                sd.resumable.map_or(Value::Null, Value::Bool),
            );
            s.insert("size".into(), sd.size.into());
        }
        self.update_totals();
        if sd.status == Status::Complete {
            {
                let mut d = self.d();
                let s = stream_mut(&mut d, i);
                s.insert("done".into(), true.into());
                s.insert("file".into(), sd.path().into());
            }
            let done = self.done_bytes();
            self.d().downloaded = done;
            return Ok(true);
        }
        self.d().downloaded = done_before + sd.downloaded;
        if sd.status == Status::Paused {
            return Ok(false);
        }
        let why = if !sd.error.is_empty() {
            sd.error.clone()
        } else if !task.error().is_empty() {
            task.error()
        } else {
            "download failed".into()
        };
        let msg = format!("{} stream: {why}", title_case(&kind));
        *lock(&self.error) = msg.clone();
        let mut d = self.d();
        d.error = msg;
        d.status = Status::Error;
        Ok(false)
    }

    async fn assemble(&self) -> Result<(), String> {
        let (variant, inputs, id, save_dir, path) = {
            let mut d = self.d();
            d.status = Status::Assembling;
            let variant: Variant = d
                .media
                .as_ref()
                .and_then(|m| m.get("variant"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or("the download has no variant")?;
            let inputs: Vec<String> = streams(&d).iter().map(|s| string(s, "file")).collect();
            (variant, inputs, d.id, d.save_dir.clone(), d.path())
        };
        self.set_text(if inputs.len() == 2 {
            "Merging video and audio..."
        } else if variant.convert == "mp3" {
            "Converting to MP3..."
        } else {
            "Finishing the file..."
        });
        let out = Path::new(&self.options.temp_dir)
            .join(format!("{}-out.{}", part_prefix(id), variant.ext))
            .to_string_lossy()
            .into_owned();
        match media::postprocess_command(self.ffmpeg.as_deref(), &inputs, &out, &variant) {
            None => {
                if inputs.len() > 1 || !variant.convert.is_empty() {
                    return Err("ffmpeg is required to finish this download".into());
                }
                let first = inputs.first().ok_or("the download has no streams")?;
                move_file(Path::new(first), Path::new(&out))?;
            }
            Some(cmd) => {
                let output = tokio::task::spawn_blocking(move || {
                    std::process::Command::new(&cmd[0]).args(&cmd[1..]).output()
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("ffmpeg failed: {e}"))?;
                if !output.status.success() {
                    let err = String::from_utf8_lossy(&output.stderr);
                    let err: Vec<char> = err.trim().chars().collect();
                    let tail: String = err[err.len().saturating_sub(300)..].iter().collect();
                    return Err(format!("ffmpeg failed: {tail}"));
                }
            }
        }
        fs::create_dir_all(&save_dir).map_err(|e| e.to_string())?;
        let target = unique_path(&path);
        move_file(Path::new(&out), Path::new(&target))?;
        let size = fs::metadata(&target).map_err(|e| e.to_string())?.len() as i64;
        let mut d = self.d();
        d.filename = Path::new(&target)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        d.size = size;
        d.downloaded = size;
        remove_parts(&self.options.temp_dir, d.id);
        d.status = Status::Complete;
        Ok(())
    }
}

fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) => {
            fs::copy(from, to).map_err(|e| e.to_string())?;
            fs::remove_file(from).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}
