//! Port of tests/test_media.py.

mod fixture;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use fixture::{FileServer, Options};
use lider_core::engine::EngineOptions;
use lider_core::http::HttpClient;
use lider_core::media::{self, MediaInfo, Resolved, Variant};
use lider_core::media_task::{MediaTask, Resolver};
use lider_core::models::{Download, Status};
use serde_json::{Map, Value};
use tokio::runtime::Handle;

fn formats() -> (Vec<Value>, f64) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/youtube_formats.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    (
        v["formats"].as_array().unwrap().clone(),
        v["duration"].as_f64().unwrap(),
    )
}

#[test]
fn youtube_links() {
    for url in [
        "https://www.youtube.com/watch?v=8h9mr7EZiA4",
        "https://youtu.be/8h9mr7EZiA4",
        "https://m.youtube.com/watch?v=8h9mr7EZiA4&t=10",
        "https://www.youtube.com/shorts/abc123",
        "https://music.youtube.com/watch?v=x",
        "http://youtube.com/live/abc",
        "  HTTPS://WWW.YOUTUBE.COM/watch?v=x  ",
    ] {
        assert!(media::is_youtube(url), "{url}");
    }
    for url in [
        "https://example.com/watch?v=1",
        "https://www.youtube.com/",
        "https://youtube.com.evil.io/watch?v=1",
        "https://www.youtube.com/channel/UC123",
    ] {
        assert!(!media::is_youtube(url), "{url}");
    }
}

fn variants() -> Vec<Variant> {
    let (f, d) = formats();
    media::build_variants(&f, d, true, true)
}

#[test]
fn one_variant_per_resolution_best_first() {
    let q: Vec<String> = variants()
        .into_iter()
        .filter(|v| v.kind == "video")
        .map(|v| v.quality)
        .collect();
    assert_eq!(
        q,
        [
            "2160p", "1440p", "1080p", "720p", "480p", "360p", "240p", "144p"
        ]
    );
}

#[test]
fn h264_mp4_preferred_then_vp9_webm() {
    let v = variants();
    let by = |q: &str| {
        v.iter()
            .find(|x| x.kind == "video" && x.quality == q)
            .unwrap()
            .clone()
    };
    let hd = by("1080p");
    assert_eq!(
        (
            hd.ext.as_str(),
            hd.video_format.as_str(),
            hd.audio_format.as_str()
        ),
        ("mp4", "137", "140")
    );
    let uhd = by("2160p");
    assert_eq!(
        (
            uhd.ext.as_str(),
            uhd.video_format.as_str(),
            uhd.audio_format.as_str()
        ),
        ("webm", "313", "251")
    );
    assert_eq!(hd.size, 92006373 + 8106621);
    assert!(hd.label.contains("1080p · MP4 (H.264)"), "{}", hd.label);
}

#[test]
fn audio_choices_skip_drc() {
    let a: Vec<(String, String, String)> = variants()
        .into_iter()
        .filter(|v| v.kind == "audio")
        .map(|v| (v.quality, v.audio_format, v.convert))
        .collect();
    let want = [
        ("M4A", "140", ""),
        ("MP3", "140", "mp3"),
        ("OPUS", "251", "opus"),
    ];
    assert_eq!(a, want.map(|(a, b, c)| (a.into(), b.into(), c.into())));
}

#[test]
fn without_ffmpeg_only_unmerged_formats() {
    let (f, d) = formats();
    let v = media::build_variants(&f, d, false, false);
    assert!(!v.is_empty());
    assert!(v.iter().all(|x| x.convert.is_empty()));
    assert!(
        v.iter()
            .all(|x| x.kind == "audio" || x.audio_format.is_empty())
    );
}

#[test]
fn labels_match_1_2() {
    // Every label as 1.2 builds it from the same format list (checked against the Python output).
    let labels: Vec<String> = variants().into_iter().map(|v| v.label).collect();
    let expected_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/variant_labels_1.2.json");
    let expected: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(expected_path).unwrap()).unwrap();
    assert_eq!(labels, expected);
}

#[test]
fn default_variant() {
    let info = MediaInfo {
        url: "u".into(),
        id: "id".into(),
        title: "t".into(),
        uploader: String::new(),
        duration: 501,
        variants: variants(),
    };
    assert_eq!(info.default_variant("video", "").unwrap().quality, "1080p");
    assert_eq!(info.default_variant("audio", "").unwrap().quality, "M4A");
    assert_eq!(
        info.default_variant("video", "480p").unwrap().quality,
        "480p"
    );
}

#[test]
fn suggested_filename_drops_trailing_dot() {
    let info = MediaInfo {
        url: "u".into(),
        id: "x".into(),
        title: "Review: A07 / A08.".into(),
        uploader: String::new(),
        duration: 1,
        variants: vec![],
    };
    assert_eq!(
        media::suggested_filename(&info, &Variant::new("video", "1080p", "", "mp4")),
        "Review: A07 _ A08.mp4"
    );
}

#[test]
fn postprocess_commands() {
    let ff = Some(Path::new("/usr/bin/ffmpeg"));
    let v = Variant::new("video", "1080p", "", "mp4");
    let two = media::postprocess_command(ff, &["v".into(), "a".into()], "o.mp4", &v).unwrap();
    assert_eq!(
        two.join(" "),
        "/usr/bin/ffmpeg -y -hide_banner -loglevel error -i v -i a -map 0:v:0 -map 1:a:0 -c copy -movflags +faststart o.mp4"
    );
    let mp3 = Variant {
        convert: "mp3".into(),
        ..Variant::new("audio", "MP3", "", "mp3")
    };
    assert!(
        media::postprocess_command(ff, &["a".into()], "o.mp3", &mp3)
            .unwrap()
            .join(" ")
            .ends_with("-i a -vn -c:a libmp3lame -q:a 2 o.mp3")
    );
    assert!(media::postprocess_command(None, &["a".into()], "o", &v).is_none());
}

#[test]
fn resolve_from_info_finds_formats() {
    let (f, d) = formats();
    let info = serde_json::json!({"formats": f, "duration": d});
    let r = media::resolve_from_info(&info, &["137".into(), "140".into()]).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].size, 92006373); // the saved list has no URLs, only sizes
    assert_eq!(r[0].ext, "mp4");
    let err = media::resolve_from_info(&info, &["999".into()]).unwrap_err();
    assert_eq!(err, "YouTube no longer offers format 999 for this video");
    assert_eq!(
        media::clean_error("ERROR: [youtube] x: Video unavailable"),
        "[youtube] x: Video unavailable"
    );
}

// ------------------------------------------------------------------ MediaTask (needs ffmpeg)

fn ffmpeg() -> Option<PathBuf> {
    media::which("ffmpeg").filter(|_| media::which("ffprobe").is_some())
}

fn make_clip(dir: &Path, kind: &str) -> Vec<u8> {
    let path = dir.join(if kind == "video" { "v.mp4" } else { "a.m4a" });
    let args: &[&str] = if kind == "video" {
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x120:rate=10:duration=3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ]
    } else {
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3",
            "-c:a",
            "aac",
        ]
    };
    let ok = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error"])
        .args(args)
        .arg(&path)
        .status()
        .unwrap();
    assert!(ok.success());
    std::fs::read(path).unwrap()
}

fn streams_of(path: &str) -> Vec<String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
            path,
        ])
        .output()
        .unwrap();
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(String::from)
        .collect();
    v.sort();
    v
}

struct MT {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    video: FileServer,
    audio: FileServer,
    resolves: Arc<AtomicUsize>,
}

async fn mt() -> MT {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let video = FileServer::start(Options {
        data: Some(make_clip(&dir, "video")),
        ..Default::default()
    })
    .await;
    let audio = FileServer::start(Options {
        data: Some(make_clip(&dir, "audio")),
        ..Default::default()
    })
    .await;
    MT {
        _tmp: tmp,
        dir,
        video,
        audio,
        resolves: Arc::new(AtomicUsize::new(0)),
    }
}

impl MT {
    fn resolver(&self) -> Resolver {
        let (v, a, n) = (
            self.video.url.clone(),
            self.audio.url.clone(),
            self.resolves.clone(),
        );
        Arc::new(move |_page: &str, ids: &[String]| {
            n.fetch_add(1, Ordering::SeqCst);
            Ok(ids
                .iter()
                .map(|i| {
                    let mut headers = Map::new();
                    headers.insert("User-Agent".into(), "test".into());
                    Resolved {
                        url: if i == "137" { v.clone() } else { a.clone() },
                        headers,
                        size: -1,
                        ext: if i == "137" {
                            "mp4".into()
                        } else {
                            "m4a".into()
                        },
                        // As yt-dlp reports for YouTube, scaled down: requests of at most 64 KiB.
                        chunk: 64 * 1024,
                    }
                })
                .collect())
        })
    }

    fn download(&self, v: Variant) -> Download {
        let info = MediaInfo {
            url: "https://www.youtube.com/watch?v=x".into(),
            id: "x".into(),
            title: "Clip".into(),
            uploader: "Me".into(),
            duration: 3,
            variants: vec![v.clone()],
        };
        let mut d = Download::new(
            1,
            &info.url,
            &format!("Clip.{}", v.ext),
            &self.dir.join("out").to_string_lossy(),
        );
        d.media = Some(media::media_record(&info, &v));
        d.size = v.size;
        d
    }

    fn task(
        &self,
        d: Download,
        resolver: Resolver,
    ) -> (MediaTask, tokio::sync::oneshot::Receiver<()>) {
        let opts = EngineOptions {
            max_connections: 4,
            retries: 1,
            retry_delay: 0.05,
            temp_dir: self.dir.join("parts").to_string_lossy().into_owned(),
            request_limit: None,
        };
        let t = MediaTask::new(
            d,
            Arc::new(|ua: &str| HttpClient::new(ua, "", 5.0)),
            opts,
            None,
            Some(resolver),
        )
        .with_ffmpeg(ffmpeg());
        let (tx, rx) = tokio::sync::oneshot::channel();
        t.on_finish(move || {
            let _ = tx.send(());
        });
        t.start(&Handle::current());
        (t, rx)
    }
}

fn merged() -> Variant {
    Variant {
        video_format: "137".into(),
        audio_format: "140".into(),
        ..Variant::new("video", "1080p", "", "mp4")
    }
}

async fn done(rx: tokio::sync::oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .unwrap()
        .unwrap();
}

macro_rules! needs_ffmpeg {
    () => {
        if ffmpeg().is_none() {
            eprintln!("skipped: ffmpeg is required");
            return;
        }
    };
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn video_and_audio_are_merged() {
    needs_ffmpeg!();
    let m = mt().await;
    let (t, rx) = m.task(m.download(merged()), m.resolver());
    done(rx).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(streams_of(&d.path()), ["audio", "video"]);
    assert_eq!(d.size as u64, std::fs::metadata(d.path()).unwrap().len());
    assert_eq!(std::fs::read_dir(m.dir.join("parts")).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn audio_converted_to_mp3() {
    needs_ffmpeg!();
    let m = mt().await;
    let v = Variant {
        audio_format: "140".into(),
        convert: "mp3".into(),
        ..Variant::new("audio", "MP3", "", "mp3")
    };
    let (t, rx) = m.task(m.download(v), m.resolver());
    done(rx).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert!(d.path().ends_with(".mp3"));
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
            &d.path(),
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "mp3");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_resume_refreshes_urls_and_keeps_progress() {
    needs_ffmpeg!();
    let m = mt().await;
    m.video.set_delay(Duration::from_millis(50));
    let (t, rx) = m.task(m.download(merged()), m.resolver());
    while t.download().downloaded == 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!t.snapshot().segments.is_empty());
    t.stop();
    done(rx).await;
    let d = t.download();
    assert_eq!(d.status, Status::Paused);
    let first = &d.media.as_ref().unwrap()["streams"][0];
    assert!(first.get("segments").is_some() || first.get("done").is_some());
    m.video.set_delay(Duration::ZERO);
    m.video.clear_requests();
    let (t2, rx2) = m.task(d, m.resolver());
    done(rx2).await;
    let d = t2.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(
        m.resolves.load(Ordering::SeqCst),
        2,
        "stream URLs must be resolved again on resume"
    );
    assert_eq!(streams_of(&d.path()), ["audio", "video"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forbidden_stream_gets_a_fresh_url() {
    needs_ffmpeg!();
    let m = mt().await;
    m.video.fail_next(1);
    let (t, rx) = m.task(m.download(merged()), m.resolver());
    done(rx).await;
    let d = t.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    assert_eq!(m.resolves.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resolver_error_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let info = MediaInfo {
        url: "u".into(),
        id: "x".into(),
        title: "t".into(),
        uploader: String::new(),
        duration: 1,
        variants: vec![],
    };
    let mut d = Download::new(1, "u", "t.mp4", &tmp.path().to_string_lossy());
    d.media = Some(media::media_record(&info, &merged()));
    let broken: Resolver = Arc::new(|_: &str, _: &[String]| {
        Err("ERROR: YouTube no longer offers format 137 for this video".into())
    });
    let t = MediaTask::new(
        d,
        Arc::new(|ua: &str| HttpClient::new(ua, "", 5.0)),
        EngineOptions {
            temp_dir: tmp.path().join("parts").to_string_lossy().into_owned(),
            ..Default::default()
        },
        None,
        Some(broken),
    );
    let (tx, rx) = tokio::sync::oneshot::channel();
    t.on_finish(move || {
        let _ = tx.send(());
    });
    t.start(&Handle::current());
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .unwrap()
        .unwrap();
    let d = t.download();
    assert_eq!(d.status, Status::Error);
    assert_eq!(
        d.error,
        "YouTube no longer offers format 137 for this video"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merging_without_ffmpeg_says_so() {
    needs_ffmpeg!(); // to make the test streams
    let m = mt().await;
    let opts = EngineOptions {
        temp_dir: m.dir.join("parts").to_string_lossy().into_owned(),
        ..Default::default()
    };
    let t = MediaTask::new(
        m.download(merged()),
        Arc::new(|ua: &str| HttpClient::new(ua, "", 5.0)),
        opts,
        None,
        Some(m.resolver()),
    )
    .with_ffmpeg(None);
    let (tx, rx) = tokio::sync::oneshot::channel();
    t.on_finish(move || {
        let _ = tx.send(());
    });
    t.start(&Handle::current());
    done(rx).await;
    let d = t.download();
    assert_eq!(
        (d.status, d.error.as_str()),
        (Status::Error, "ffmpeg is required to finish this download")
    );
}

#[test]
fn live_probe_real_video() {
    if std::env::var_os("LIDER_NETWORK_TESTS").is_none() {
        eprintln!("skipped: set LIDER_NETWORK_TESTS=1 to query YouTube");
        return;
    }
    let info = media::probe("https://www.youtube.com/watch?v=8h9mr7EZiA4").unwrap();
    assert!(!info.title.is_empty());
    assert!(info.variants.iter().any(|v| v.kind == "video"));
}

#[test]
fn the_package_copy_of_yt_dlp_sits_next_to_the_program() {
    // deb: /usr/bin/lider; AppImage: $APPDIR/usr/bin/lider.
    assert_eq!(
        lider_core::media::bundled_beside(Path::new("/usr/bin/lider")),
        Path::new("/usr/lib/lider/yt-dlp")
    );
    assert_eq!(
        lider_core::media::bundled_beside(Path::new("/tmp/.mount_Lider/usr/bin/lider")),
        Path::new("/tmp/.mount_Lider/usr/lib/lider/yt-dlp")
    );
}
