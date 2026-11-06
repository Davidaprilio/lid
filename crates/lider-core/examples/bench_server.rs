//! A fast range-capable HTTP server for throughput benchmarks: serves N MiB of data from memory.
//!
//!     cargo run --release -p lider-core --example bench_server -- 1024 8099 [MB/s per connection]

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mib: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1024);
    let port: u16 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(8099);
    // Optional cap per connection, like a real server: shows what several connections buy.
    let cap: f64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(0.0);
    let data: Arc<Vec<u8>> = Arc::new(
        (0..mib * 1024 * 1024)
            .map(|i| (i * 31 % 251) as u8)
            .collect(),
    );
    let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    eprintln!("serving {mib} MiB on http://127.0.0.1:{port}/file.bin");
    loop {
        let (sock, _) = listener.accept().await.unwrap();
        let data = data.clone();
        tokio::spawn(async move {
            let mut r = BufReader::new(sock);
            let mut range = String::new();
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).await.unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if line.to_ascii_lowercase().starts_with("range:") {
                    range = line[6..].trim().to_string();
                }
            }
            let mut sock = r.into_inner();
            let total = data.len();
            let start = range
                .strip_prefix("bytes=")
                .and_then(|r| r.split('-').next())
                .and_then(|s| s.parse::<usize>().ok());
            let (status, from) = match start {
                Some(s) => ("206 Partial Content", s),
                None => ("200 OK", 0),
            };
            let mut head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
                total - from
            );
            if start.is_some() {
                head += &format!("Content-Range: bytes {from}-{}/{total}\r\n", total - 1);
            }
            head += "\r\n";
            if sock.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            for chunk in data[from..].chunks(256 * 1024) {
                if sock.write_all(chunk).await.is_err() {
                    return;
                }
                if cap > 0.0 {
                    tokio::time::sleep(std::time::Duration::from_secs_f64(
                        chunk.len() as f64 / (cap * 1e6),
                    ))
                    .await;
                }
            }
        });
    }
}
