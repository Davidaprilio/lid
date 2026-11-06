//! YouTube support: detect a link, list the variants, resolve fresh stream URLs (`media.py` in 1.x).
//!
//! yt-dlp is used only to read the video page (title, formats, direct stream URLs). The streams themselves
//! are fetched by LID's multi-connection engine (see `media_task`) and joined or converted with ffmpeg.
//!
//! 1.x imported yt-dlp as a Python module. 2.x runs it as a program (`yt-dlp --dump-single-json`). It looks,
//! in this order, for the user's own copy in `~/.local/bin` (pip --user or a download, so it can be updated
//! without waiting for LID), the copy a package ships (`<prefix>/lib/lider/yt-dlp` next to
//! `<prefix>/bin/lider`: deb and AppImage), PATH (the distribution's), and `python3 -m yt_dlp`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::names::safe_filename;

static YOUTUBE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^https?://(?:(?:www|m|music)\.)?(?:youtube\.com/(?:watch\?|shorts/|live/|embed/)|youtu\.be/)\S+",
    )
    .unwrap()
});
static QUALITY_NOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{3,4}p)").unwrap());

pub const AUDIO_CHOICES: [&str; 3] = ["m4a", "mp3", "opus"];

pub fn is_youtube(url: &str) -> bool {
    YOUTUBE_RE.is_match(url.trim())
}

// --------------------------------------------------------------------------- tools

/// A program on PATH (`shutil.which`); on Windows `name.exe` (and `.cmd`, `.bat`) as well.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
            name.to_string(),
        ]
    } else {
        vec![name.to_string()]
    };
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)).collect::<Vec<_>>())
        .find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    crate::fsio::is_executable(p)
}

/// How to run yt-dlp: the program and the arguments that come before LID's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtDlp {
    pub program: PathBuf,
    pub prefix: Vec<String>,
}

impl YtDlp {
    fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.prefix);
        c
    }
}

/// The copy shipped next to the program: `<prefix>/bin/lider` → `<prefix>/lib/lider/yt-dlp`.
pub fn bundled_beside(exe: &Path) -> PathBuf {
    exe.parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new("/"))
        .join("lib/lider/yt-dlp")
}

fn find_yt_dlp() -> Option<YtDlp> {
    let plain = |program: PathBuf| YtDlp {
        program,
        prefix: vec![],
    };
    let local = crate::paths::home_dir().join(".local/bin/yt-dlp");
    if is_executable(&local) {
        return Some(plain(local));
    }
    // The copy LID downloaded when YouTube first needed it (crate::tools).
    let managed = crate::tools::yt_dlp_path(&crate::paths::Dirs::from_env());
    if is_executable(&managed) {
        return Some(plain(managed));
    }
    if let Ok(exe) = std::env::current_exe() {
        let shipped = bundled_beside(&exe);
        if is_executable(&shipped) {
            return Some(plain(shipped));
        }
    }
    if let Some(p) = which("yt-dlp") {
        return Some(plain(p));
    }
    let python = which("python3")?;
    let ok = Command::new(&python)
        .args(["-c", "import yt_dlp"])
        .output()
        .is_ok_and(|o| o.status.success());
    ok.then(|| YtDlp {
        program: python,
        prefix: vec!["-m".into(), "yt_dlp".into()],
    })
}

static FOUND_YT_DLP: std::sync::Mutex<Option<Option<YtDlp>>> = std::sync::Mutex::new(None);

/// yt-dlp, looked up once (again after [`forget_yt_dlp`]).
pub fn yt_dlp() -> Option<YtDlp> {
    let mut found = FOUND_YT_DLP.lock().unwrap_or_else(|e| e.into_inner());
    found.get_or_insert_with(find_yt_dlp).clone()
}

/// Look again next time (after LID downloaded yt-dlp).
pub fn forget_yt_dlp() {
    *FOUND_YT_DLP.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// yt-dlp can be used; otherwise the UI offers to download it (crate::tools).
pub fn available() -> bool {
    yt_dlp().is_some()
}

/// `ffmpeg` on PATH, looked up once.
pub fn ffmpeg() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND.get_or_init(|| which("ffmpeg")).as_deref()
}

/// ffmpeg was built with the MP3 encoder.
pub fn ffmpeg_has_mp3() -> bool {
    static HAS: OnceLock<bool> = OnceLock::new();
    *HAS.get_or_init(|| {
        let Some(exe) = ffmpeg() else { return false };
        run_with_timeout(
            Command::new(exe).args(["-hide_banner", "-encoders"]),
            std::time::Duration::from_secs(10),
        )
        .is_some_and(|out| out.contains("libmp3lame"))
    })
}

fn run_with_timeout(cmd: &mut Command, timeout: std::time::Duration) -> Option<String> {
    use std::process::Stdio;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut s);
        s
    });
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return reader.join().ok(),
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// The JavaScript runtime YouTube needs to unlock its streams: the first of deno, node, bun.
pub fn js_runtime() -> Option<(&'static str, PathBuf)> {
    ["deno", "node", "bun"]
        .into_iter()
        .find_map(|name| which(name).map(|p| (name, p)))
}

fn yt_dlp_args() -> Vec<String> {
    let mut args: Vec<String> = [
        "--dump-single-json",
        "--skip-download",
        "--no-playlist",
        "--quiet",
        "--no-warnings",
        "--no-progress",
    ]
    .map(String::from)
    .to_vec();
    if let Some((name, path)) = js_runtime() {
        args.push("--js-runtimes".into());
        args.push(format!("{name}:{}", path.display()));
    }
    args
}

/// The page's information as yt-dlp's JSON. Errors lose yt-dlp's "ERROR: " prefix.
pub fn extract_info(url: &str) -> Result<Value, String> {
    let ytdlp = yt_dlp().ok_or("needs the yt-dlp package: pip install yt-dlp")?;
    let out = ytdlp
        .command()
        .args(yt_dlp_args())
        .arg("--")
        .arg(url)
        .output()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("yt-dlp failed");
        return Err(clean_error(line));
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("yt-dlp answered something unexpected: {e}"))
}

/// yt-dlp's message without the "ERROR: " prefix.
pub fn clean_error(text: &str) -> String {
    text.replace("ERROR: ", "").trim().to_string()
}

// --------------------------------------------------------------------------- variants

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    /// video | audio
    pub kind: String,
    /// "1080p", "720p60", "M4A"…
    pub quality: String,
    pub label: String,
    /// Extension of the saved file.
    pub ext: String,
    #[serde(default)]
    pub video_format: String,
    #[serde(default)]
    pub audio_format: String,
    #[serde(default = "minus_one")]
    pub size: i64,
    #[serde(default)]
    pub height: i64,
    /// mp3 | opus | "" (remux only)
    #[serde(default)]
    pub convert: String,
}

fn minus_one() -> i64 {
    -1
}

impl Variant {
    pub fn new(kind: &str, quality: &str, label: &str, ext: &str) -> Variant {
        Variant {
            kind: kind.into(),
            quality: quality.into(),
            label: label.into(),
            ext: ext.into(),
            video_format: String::new(),
            audio_format: String::new(),
            size: -1,
            height: 0,
            convert: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub url: String,
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration: i64,
    pub variants: Vec<Variant>,
}

impl MediaInfo {
    /// The variant to preselect: the one of that quality, else for video the best H.264 MP4 up to 1080p
    /// (it plays everywhere), else the first.
    pub fn default_variant(&self, kind: &str, quality: &str) -> Option<&Variant> {
        let of_kind: Vec<&Variant> = self.variants.iter().filter(|v| v.kind == kind).collect();
        let options: Vec<&Variant> = if of_kind.is_empty() {
            self.variants.iter().collect()
        } else {
            of_kind
        };
        let first = *options.first()?;
        if let Some(v) = options.iter().find(|v| v.quality == quality) {
            return Some(v);
        }
        if kind == "video" {
            return Some(
                options
                    .iter()
                    .find(|v| v.height <= 1080 && v.ext == "mp4")
                    .copied()
                    .unwrap_or(first),
            );
        }
        Some(first)
    }
}

// JSON access with Python's `dict.get(...) or default` truthiness.
fn get<'a>(f: &'a Value, key: &str) -> Option<&'a Value> {
    f.get(key).filter(|v| crate::pyutil::truthy(v))
}

fn num(f: &Value, key: &str) -> Option<f64> {
    get(f, key).and_then(Value::as_f64)
}

fn text(f: &Value, key: &str) -> String {
    match get(f, key) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

/// `str(f.get(key))`: "None" for a missing or null value, as Python prints it.
fn py_str(f: &Value, key: &str) -> String {
    match f.get(key) {
        None | Some(Value::Null) => "None".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.into(),
        Some(v) => v.to_string(),
    }
}

fn is_none(f: &Value, key: &str) -> bool {
    matches!(f.get(key), None | Some(Value::Null))
}

fn codec_is_none(f: &Value, key: &str) -> bool {
    is_none(f, key) || f.get(key).and_then(Value::as_str) == Some("none")
}

/// Bytes of a format: its size, else bitrate × duration, else -1.
fn size_of(f: &Value, duration: f64) -> i64 {
    if let Some(n) = num(f, "filesize").or_else(|| num(f, "filesize_approx")) {
        return n as i64;
    }
    match num(f, "tbr") {
        Some(tbr) if duration != 0.0 => (tbr * 1000.0 / 8.0 * duration) as i64,
        _ => -1,
    }
}

fn usable(f: &Value) -> bool {
    text(f, "protocol").starts_with("http")
        && !f
            .get("format_id")
            .map(|_| py_str(f, "format_id"))
            .unwrap_or_default()
            .contains("drc")
}

fn quality(f: &Value) -> String {
    let note = text(f, "format_note");
    let base = match QUALITY_NOTE.captures(&note) {
        Some(m) => m[1].to_string(),
        None => format!("{}p", py_int_str(f, "height")),
    };
    match num(f, "fps") {
        Some(fps) if fps > 30.0 => format!("{base}{}", fps as i64),
        _ => base,
    }
}

/// `f"{f.get('height')}p"`: Python prints an int height as "1080" and a missing one as "None".
fn py_int_str(f: &Value, key: &str) -> String {
    match f.get(key) {
        Some(Value::Number(n)) if n.is_f64() => {
            let x = n.as_f64().unwrap();
            if x.fract() == 0.0 {
                format!("{x:.1}")
            } else {
                x.to_string()
            }
        }
        _ => py_str(f, key),
    }
}

fn codec(f: &Value) -> String {
    let v = text(f, "vcodec");
    if v.starts_with("avc1") {
        "H.264".into()
    } else if v.starts_with("vp9") || v.starts_with("vp09") {
        "VP9".into()
    } else if v.starts_with("av01") {
        "AV1".into()
    } else {
        v.split('.').next().unwrap_or("").to_uppercase()
    }
}

/// "92 KB", "87.7 MB", "1.25 GB", or "size unknown".
pub fn fmt_size(n: i64) -> String {
    if n < 0 {
        return "size unknown".into();
    }
    let n = n as f64;
    if n < 1024.0 * 1024.0 {
        format!("{:.0} KB", n / 1024.0)
    } else if n < 1024f64.powi(3) {
        format!("{:.1} MB", n / 1024f64.powi(2))
    } else {
        format!("{:.2} GB", n / 1024f64.powi(3))
    }
}

/// The first format with the highest key, like Python's `max`.
fn max_first<'a>(fs: &[&'a Value], key: impl Fn(&Value) -> f64) -> Option<&'a Value> {
    let mut best: Option<&Value> = None;
    for f in fs {
        if best.is_none_or(|b| key(f) > key(b)) {
            best = Some(f);
        }
    }
    best
}

fn format_id(f: &Value) -> String {
    py_str(f, "format_id")
}

/// The choices offered for a video: one per resolution (best first), then the audio-only ones.
pub fn build_variants(
    formats: &[Value],
    duration: f64,
    have_ffmpeg: bool,
    have_mp3: bool,
) -> Vec<Variant> {
    let usable: Vec<&Value> = formats.iter().filter(|f| usable(f)).collect();
    let audio: Vec<&Value> = usable
        .iter()
        .copied()
        .filter(|f| {
            f.get("vcodec").and_then(Value::as_str) == Some("none") && !codec_is_none(f, "acodec")
        })
        .collect();
    let bitrate = |f: &Value| num(f, "abr").or_else(|| num(f, "tbr")).unwrap_or(0.0);
    let ext_is =
        |e: &'static str| move |f: &&Value| f.get("ext").and_then(Value::as_str) == Some(e);
    let m4a = max_first(
        &audio
            .iter()
            .copied()
            .filter(ext_is("m4a"))
            .collect::<Vec<_>>(),
        bitrate,
    );
    let opus = max_first(
        &audio
            .iter()
            .copied()
            .filter(ext_is("webm"))
            .collect::<Vec<_>>(),
        bitrate,
    );
    let mut out: Vec<Variant> = Vec::new();

    if have_ffmpeg {
        let mut by_quality: indexmap::IndexMap<String, Vec<&Value>> = indexmap::IndexMap::new();
        for f in usable.iter().copied().filter(|f| {
            !codec_is_none(f, "vcodec")
                && f.get("acodec").and_then(Value::as_str) == Some("none")
                && get(f, "height").is_some()
        }) {
            by_quality.entry(quality(f)).or_default().push(f);
        }
        let rank = |f: &Value| match codec(f).as_str() {
            "H.264" => 0,
            "VP9" => 1,
            "AV1" => 2,
            _ => 9,
        };
        for (q, fs) in &by_quality {
            // min by (codec rank, -bitrate), the first on a tie.
            let mut f = fs[0];
            for g in &fs[1..] {
                let (a, b) = (
                    (rank(g), -num(g, "tbr").unwrap_or(0.0)),
                    (rank(f), -num(f, "tbr").unwrap_or(0.0)),
                );
                if a.0 < b.0 || (a.0 == b.0 && a.1 < b.1) {
                    f = g;
                }
            }
            let c = codec(f);
            let (audio_f, ext) = match (m4a, opus) {
                (Some(m), _) if c == "H.264" || c == "AV1" => (m, "mp4"),
                (_, Some(o)) => (o, "webm"),
                (Some(m), None) => (m, "mkv"),
                (None, None) => continue,
            };
            let size = size_of(f, duration);
            let a_size = size_of(audio_f, duration);
            let total = if size >= 0 && a_size >= 0 {
                size + a_size
            } else {
                -1
            };
            out.push(Variant {
                kind: "video".into(),
                quality: q.clone(),
                label: format!("{q} · {} ({c}) · {}", ext.to_uppercase(), fmt_size(total)),
                ext: ext.into(),
                video_format: format_id(f),
                audio_format: format_id(audio_f),
                size: total,
                height: num(f, "height").unwrap_or(0.0) as i64,
                convert: String::new(),
            });
        }
    } else {
        for f in usable.iter().copied() {
            if !codec_is_none(f, "vcodec") && !codec_is_none(f, "acodec") {
                let q = quality(f);
                let size = size_of(f, duration);
                let ext = py_str(f, "ext");
                out.push(Variant {
                    kind: "video".into(),
                    label: format!("{q} · {} · {}", ext.to_uppercase(), fmt_size(size)),
                    quality: q,
                    ext,
                    video_format: format_id(f),
                    audio_format: String::new(),
                    size,
                    height: num(f, "height").unwrap_or(0.0) as i64,
                    convert: String::new(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| a.quality.cmp(&b.quality))
    });

    if let Some(m) = m4a {
        let size = size_of(m, duration);
        let kbps = num(m, "abr").unwrap_or(0.0) as i64;
        out.push(Variant {
            audio_format: format_id(m),
            size,
            ..Variant::new(
                "audio",
                "M4A",
                &format!("M4A (AAC {kbps} kbps) · {}", fmt_size(size)),
                "m4a",
            )
        });
        if have_mp3 {
            let est = if duration != 0.0 {
                (duration * 192_000.0 / 8.0) as i64
            } else {
                -1
            };
            out.push(Variant {
                audio_format: format_id(m),
                size: est,
                convert: "mp3".into(),
                ..Variant::new(
                    "audio",
                    "MP3",
                    &format!("MP3 (192 kbps, converted) · ~{}", fmt_size(est)),
                    "mp3",
                )
            });
        }
    }
    if let (Some(o), true) = (opus, have_ffmpeg) {
        let size = size_of(o, duration);
        let kbps = num(o, "abr").unwrap_or(0.0) as i64;
        out.push(Variant {
            audio_format: format_id(o),
            size,
            convert: "opus".into(),
            ..Variant::new(
                "audio",
                "OPUS",
                &format!("Opus ({kbps} kbps) · {}", fmt_size(size)),
                "opus",
            )
        });
    }
    out
}

/// Read a video page and list its variants.
pub fn probe(url: &str) -> Result<MediaInfo, String> {
    let info = extract_info(url)?;
    let duration = num(&info, "duration").unwrap_or(0.0);
    let formats = info
        .get("formats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let title = text(&info, "title");
    Ok(MediaInfo {
        url: get(&info, "webpage_url")
            .and_then(Value::as_str)
            .unwrap_or(url)
            .to_string(),
        id: text(&info, "id"),
        title: if title.is_empty() {
            "video".into()
        } else {
            title
        },
        uploader: {
            let u = text(&info, "uploader");
            if u.is_empty() {
                text(&info, "channel")
            } else {
                u
            }
        },
        duration: duration as i64,
        variants: build_variants(&formats, duration, ffmpeg().is_some(), ffmpeg_has_mp3()),
    })
}

/// "Title.mp4", safe as a file name, without trailing dots or spaces before the extension.
pub fn suggested_filename(info: &MediaInfo, v: &Variant) -> String {
    let base = safe_filename(&info.title);
    format!("{}.{}", base.trim_end_matches(['.', ' ']), v.ext)
}

/// The `Download::media` value saved for a chosen variant.
pub fn media_record(info: &MediaInfo, v: &Variant) -> Map<String, Value> {
    let mut streams = vec![];
    if !v.video_format.is_empty() {
        streams.push(json!({"kind": "video", "format_id": v.video_format}));
    }
    if !v.audio_format.is_empty() {
        streams.push(json!({"kind": "audio", "format_id": v.audio_format}));
    }
    let Value::Object(m) = json!({
        "source": "youtube",
        "page_url": info.url,
        "video_id": info.id,
        "title": info.title,
        "uploader": info.uploader,
        "variant": v,
        "streams": streams,
    }) else {
        unreachable!()
    };
    m
}

/// A freshly resolved stream: direct URL (it expires after a few hours), headers, size, extension.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub url: String,
    pub headers: Map<String, Value>,
    pub size: i64,
    pub ext: String,
}

/// Fresh direct URLs for the given format ids.
pub fn resolve_streams(page_url: &str, format_ids: &[String]) -> Result<Vec<Resolved>, String> {
    let info = extract_info(page_url)?;
    resolve_from_info(&info, format_ids)
}

pub fn resolve_from_info(info: &Value, format_ids: &[String]) -> Result<Vec<Resolved>, String> {
    let duration = num(info, "duration").unwrap_or(0.0);
    let formats: Vec<&Value> = info
        .get("formats")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    format_ids
        .iter()
        .map(|fid| {
            let f = formats
                .iter()
                .find(|f| format_id(f) == *fid)
                .ok_or_else(|| format!("YouTube no longer offers format {fid} for this video"))?;
            Ok(Resolved {
                url: text(f, "url"),
                headers: get(f, "http_headers")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
                size: if get(f, "filesize").is_some() {
                    size_of(f, duration)
                } else {
                    -1
                },
                ext: {
                    let e = text(f, "ext");
                    if e.is_empty() { "bin".into() } else { e }
                },
            })
        })
        .collect()
}

/// The ffmpeg command that turns the downloaded streams into the final file, or `None` (no ffmpeg) to
/// just move the single stream.
pub fn postprocess_command(
    ffmpeg: Option<&Path>,
    inputs: &[String],
    output: &str,
    variant: &Variant,
) -> Option<Vec<String>> {
    let exe = ffmpeg?.to_string_lossy().into_owned();
    let mut cmd: Vec<String> = [exe.as_str(), "-y", "-hide_banner", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    if inputs.len() == 2 {
        cmd.extend(s(&[
            "-i", &inputs[0], "-i", &inputs[1], "-map", "0:v:0", "-map", "1:a:0", "-c", "copy",
        ]));
        if variant.ext == "mp4" {
            cmd.extend(s(&["-movflags", "+faststart"]));
        }
    } else if variant.convert == "mp3" {
        cmd.extend(s(&[
            "-i",
            &inputs[0],
            "-vn",
            "-c:a",
            "libmp3lame",
            "-q:a",
            "2",
        ]));
    } else if variant.convert == "opus" {
        cmd.extend(s(&["-i", &inputs[0], "-vn", "-c:a", "copy"]));
    } else {
        // YouTube's M4A/MP4 streams are fragmented DASH files; a stream copy makes them regular files.
        cmd.extend(s(&["-i", &inputs[0], "-c", "copy"]));
    }
    cmd.push(output.to_string());
    Some(cmd)
}
