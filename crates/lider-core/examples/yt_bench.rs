//! A YouTube stream's speed through LID's engine, without and with pieces (`request_limit`), and through
//! yt-dlp on its own, for the same stream for the same time. Needs network and yt-dlp.
//!
//!     ROUNDS=3 cargo run --release -p lider-core --example yt_bench -- <page url> [seconds] [connections]
use std::path::Path;
use std::time::{Duration, Instant};

use lider_core::engine::{DownloadTask, EngineOptions};
use lider_core::http::HttpClient;
use lider_core::media;
use lider_core::models::Download;
use serde_json::Value;
use tokio::runtime::Handle;

fn mb_per_s(bytes: i64, secs: f64) -> String {
    format!(
        "{:.2} MB/s ({} MB in {secs:.0} s)",
        bytes as f64 / secs / 1e6,
        bytes / 1_000_000
    )
}

async fn engine_run(
    r: &media::Resolved,
    dir: &Path,
    secs: u64,
    connections: i64,
    limit: Option<i64>,
) -> i64 {
    let mut d = Download::new(1, &r.url, "bench.bin", &dir.to_string_lossy());
    d.size = r.size;
    d.headers = r
        .headers
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    let ua = d.headers.get("User-Agent").cloned().unwrap_or_default();
    let opts = EngineOptions {
        max_connections: connections,
        temp_dir: dir.join("parts").to_string_lossy().into_owned(),
        request_limit: limit,
        ..Default::default()
    };
    let t = DownloadTask::new(d, HttpClient::new(&ua, "", 30.0).unwrap(), opts, None, None);
    t.start(&Handle::current());
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end && t.is_running() {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let got = t.download().downloaded;
    t.stop();
    while t.is_running() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = std::fs::remove_dir_all(dir);
    got
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let page = a
        .get(1)
        .expect("usage: yt_bench <page url> [seconds] [connections]");
    let secs: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let connections: i64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);
    let info = media::extract_info(page).expect("yt-dlp could not read the page");
    // The largest video-only stream over plain HTTPS (what LID downloads for a high resolution).
    let size = |f: &Value| {
        f["filesize"]
            .as_i64()
            .or(f["filesize_approx"].as_i64())
            .unwrap_or(0)
    };
    let best = info["formats"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["protocol"] == "https" && f["vcodec"] != "none" && f["acodec"] == "none")
        .max_by_key(|f| size(f))
        .expect("no video stream");
    let fid = best["format_id"].as_str().unwrap().to_string();
    let r = media::resolve_from_info(&info, std::slice::from_ref(&fid))
        .unwrap()
        .remove(0);
    println!(
        "format {fid} ({}), {} MB, yt-dlp piece size {} bytes, {connections} connections, {secs} s each",
        best["format_note"].as_str().unwrap_or(""),
        r.size / 1_000_000,
        r.chunk
    );
    let rounds: usize = std::env::var("ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let tmp = std::env::temp_dir().join(format!("yt-bench-{}", std::process::id()));
    let piece = r.chunk.max(1 << 20);
    let mut totals = [0i64; 3];
    // Interleaved rounds, so a line that changes speed weighs on every method alike.
    for round in 1..=rounds {
        let open = engine_run(&r, &tmp.join("a"), secs, connections, None).await;
        let pieces = engine_run(&r, &tmp.join("b"), secs, connections, Some(piece)).await;
        let out = tmp.join("c.bin");
        let _ = std::fs::create_dir_all(&tmp);
        let yt = media::yt_dlp().expect("no yt-dlp");
        let mut cmd = std::process::Command::new(&yt.program);
        cmd.args(&yt.prefix)
            .args(["-q", "--no-warnings", "--no-part", "-f", &fid, "-o"])
            .arg(&out);
        if let Some((name, path)) = media::js_runtime() {
            cmd.arg("--js-runtimes")
                .arg(format!("{name}:{}", path.display()));
        }
        let mut child = cmd.arg(page).spawn().unwrap();
        std::thread::sleep(Duration::from_secs(secs));
        let _ = child.kill();
        let _ = child.wait();
        let ytdlp = std::fs::metadata(&out).map_or(0, |m| m.len() as i64);
        let _ = std::fs::remove_dir_all(&tmp);
        println!(
            "round {round}: open-ended {} | pieces of {} MB {} | yt-dlp {}",
            mb_per_s(open, secs as f64),
            piece >> 20,
            mb_per_s(pieces, secs as f64),
            mb_per_s(ytdlp, secs as f64)
        );
        for (t, v) in totals.iter_mut().zip([open, pieces, ytdlp]) {
            *t += v;
        }
    }
    let all = (secs as usize * rounds) as f64;
    println!(
        "average: LID open-ended {} | LID pieces {} | yt-dlp {}",
        mb_per_s(totals[0], all),
        mb_per_s(totals[1], all),
        mb_per_s(totals[2], all)
    );
}
