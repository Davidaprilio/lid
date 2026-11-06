//! Download a URL with the engine and print the throughput.
//!
//!     cargo run --release -p lider-core --example bench_download -- URL CONNECTIONS DIR

use std::time::Instant;

use lider_core::engine::{DownloadTask, EngineOptions};
use lider_core::http::HttpClient;
use lider_core::models::{Download, Status};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (url, conns, dir) = (&a[1], a[2].parse().unwrap(), &a[3]);
    let d = Download::new(1, url, "bench.bin", dir);
    let opts = EngineOptions {
        max_connections: conns,
        temp_dir: format!("{dir}/parts"),
        ..Default::default()
    };
    let task = DownloadTask::new(
        d,
        HttpClient::new("bench", "", 30.0).unwrap(),
        opts,
        None,
        None,
    );
    let (tx, rx) = tokio::sync::oneshot::channel();
    task.on_finish(move || {
        let _ = tx.send(());
    });
    let t0 = Instant::now();
    task.start(&tokio::runtime::Handle::current());
    rx.await.unwrap();
    let secs = t0.elapsed().as_secs_f64();
    let d = task.download();
    assert_eq!(d.status, Status::Complete, "{}", d.error);
    println!(
        "rust  conns={conns:>2}  {:.0} MB/s  ({:.2} s)",
        d.size as f64 / secs / 1e6,
        secs
    );
    std::fs::remove_file(d.path()).unwrap();
}
