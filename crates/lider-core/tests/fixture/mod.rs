//! Local servers for the engine tests (port of tests/httpfixture.py, plus a small FTP server).
//!
//! The HTTP server serves one file at any path, with ranges, throttling and faults:
//! - `ranges: false` ignores the Range header (always 200, whole file);
//! - `length: false` sends no Content-Length on a 200 and closes the connection at the end;
//! - `delay_per_chunk` sleeps after every 32 KiB;
//! - `drop_after` makes the first response stop after that many bytes;
//! - `fail_first` answers the first N requests with 403;
//! - `/redirect/<n>/…` redirects n times before serving, `/loop` redirects forever, `…/hang` never answers;
//! - a request line with an absolute URL (as sent to a proxy) is served like any other path.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub struct Options {
    pub size: usize,
    pub ranges: bool,
    pub length: bool,
    pub delay_per_chunk: Duration,
    pub drop_after: usize,
    pub fail_first: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            size: 3 * 1024 * 1024,
            ranges: true,
            length: true,
            delay_per_chunk: Duration::ZERO,
            drop_after: 0,
            fail_first: 0,
        }
    }
}

pub struct Request {
    pub path: String,
    /// The Range header, "" when absent.
    pub range: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub struct Inner {
    pub data: Vec<u8>,
    pub ranges: AtomicBool,
    /// Requests that still get range support; -1 = all.
    pub ranged_left: AtomicIsize,
    pub length: bool,
    pub delay_ms: AtomicUsize,
    pub drop_after: usize,
    pub dropped: AtomicBool,
    pub fail_first: AtomicUsize,
    pub requests: Mutex<Vec<Request>>,
}

pub struct FileServer {
    pub inner: Arc<Inner>,
    pub url: String,
    pub base: String,
    pub sha256: String,
}

pub fn random_bytes(n: usize) -> Vec<u8> {
    // xorshift: fast, deterministic per size, good enough to catch misplaced bytes.
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15 ^ n as u64;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

impl FileServer {
    pub async fn start(o: Options) -> FileServer {
        let data = random_bytes(o.size);
        let sha256 = sha256_hex(&data);
        let inner = Arc::new(Inner {
            data,
            ranges: AtomicBool::new(o.ranges),
            ranged_left: AtomicIsize::new(-1),
            length: o.length,
            delay_ms: AtomicUsize::new(o.delay_per_chunk.as_micros() as usize),
            drop_after: o.drop_after,
            dropped: AtomicBool::new(false),
            fail_first: AtomicUsize::new(o.fail_first),
            requests: Mutex::new(Vec::new()),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let srv = inner.clone();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    return;
                };
                let srv = srv.clone();
                tokio::spawn(async move {
                    let _ = serve(srv, sock).await;
                });
            }
        });
        let base = format!("http://127.0.0.1:{port}");
        FileServer {
            url: format!("{base}/files/test%20file.bin"),
            base,
            inner,
            sha256,
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.inner.data
    }

    pub fn ranges(&self) -> Vec<String> {
        self.inner
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.range.clone())
            .collect()
    }

    pub fn clear_requests(&self) {
        self.inner.requests.lock().unwrap().clear();
    }

    pub fn set_delay(&self, d: Duration) {
        self.inner
            .delay_ms
            .store(d.as_micros() as usize, Ordering::Relaxed);
    }

    /// Only the next `n` requests get range support (to fake a server that changes its mind).
    pub fn ranges_for_next(&self, n: isize) {
        self.inner.ranged_left.store(n, Ordering::SeqCst);
    }

    pub fn set_ranges(&self, on: bool) {
        self.inner.ranges.store(on, Ordering::Relaxed);
    }
}

async fn serve(srv: Arc<Inner>, sock: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(sock);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).await? == 0 || h == "\r\n" {
            break;
        }
        if let Some((k, v)) = h.trim_end().split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let range = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("range"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    srv.requests.lock().unwrap().push(Request {
        path: path.clone(),
        range: range.clone(),
        headers,
    });
    let mut sock = reader.into_inner();

    let simple = |status: &str, extra: &str| {
        format!("HTTP/1.1 {status}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n")
    };
    if path.ends_with("/hang") {
        tokio::time::sleep(Duration::from_secs(60)).await;
        return Ok(());
    }
    if path == "/loop" {
        return sock
            .write_all(simple("302 Found", "Location: /loop\r\n").as_bytes())
            .await;
    }
    if let Some(rest) = path.strip_prefix("/redirect/") {
        let (n, tail) = rest.split_once('/').unwrap_or((rest, ""));
        let n: usize = n.parse().unwrap_or(0);
        let next = if n > 1 {
            format!("/redirect/{}/{tail}", n - 1)
        } else {
            format!("/{tail}")
        };
        return sock
            .write_all(simple("302 Found", &format!("Location: {next}\r\n")).as_bytes())
            .await;
    }
    if path.starts_with("/status/404") {
        return sock.write_all(simple("404 Not Found", "").as_bytes()).await;
    }
    if srv.fail_first.load(Ordering::SeqCst) > 0 {
        srv.fail_first.fetch_sub(1, Ordering::SeqCst);
        return sock.write_all(simple("403 Forbidden", "").as_bytes()).await;
    }

    let total = srv.data.len();
    let (mut start, mut end) = (0usize, total.saturating_sub(1));
    let parsed = range.strip_prefix("bytes=").and_then(|r| {
        let (a, b) = r.split_once('-')?;
        Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()))
    });
    let mut honour = srv.ranges.load(Ordering::Relaxed);
    if honour && parsed.is_some() {
        let left = srv.ranged_left.load(Ordering::SeqCst);
        if left == 0 {
            honour = false;
        } else if left > 0 {
            srv.ranged_left.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let partial = parsed.is_some() && honour;
    if partial {
        let (a, b) = parsed.unwrap();
        start = a;
        if let Some(b) = b {
            end = b;
        }
        if start >= total {
            let head = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            return sock.write_all(head.as_bytes()).await;
        }
    }
    let body = &srv.data[start..=end.min(total - 1)];
    let mut head = format!(
        "HTTP/1.1 {}\r\n",
        if partial {
            "206 Partial Content"
        } else {
            "200 OK"
        }
    );
    if partial {
        head += &format!("Content-Range: bytes {start}-{end}/{total}\r\n");
    }
    if srv.length || partial {
        head += &format!("Content-Length: {}\r\n", body.len());
    }
    head +=
        "Content-Disposition: attachment; filename=\"test file.bin\"\r\nConnection: close\r\n\r\n";
    sock.write_all(head.as_bytes()).await?;
    let drop = srv.drop_after > 0 && !srv.dropped.load(Ordering::SeqCst);
    let mut sent = 0;
    for chunk in body.chunks(32 * 1024) {
        if drop && sent + chunk.len() > srv.drop_after {
            srv.dropped.store(true, Ordering::SeqCst);
            sock.write_all(&chunk[..srv.drop_after - sent]).await?;
            return Ok(());
        }
        sock.write_all(chunk).await?;
        sent += chunk.len();
        let delay = srv.delay_ms.load(Ordering::Relaxed);
        if delay > 0 {
            tokio::time::sleep(Duration::from_micros(delay as u64)).await;
        }
    }
    sock.shutdown().await
}

/// A one-file FTP server: anonymous or any login, passive mode, `SIZE` and `RETR` of `/pub/<name>`.
pub struct FtpServer {
    pub url: String,
    pub data: Arc<Vec<u8>>,
    pub sha256: String,
    pub logins: Arc<Mutex<Vec<String>>>,
}

impl FtpServer {
    pub async fn start(size: usize) -> FtpServer {
        let data = Arc::new(random_bytes(size));
        let sha256 = sha256_hex(&data);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let logins = Arc::new(Mutex::new(Vec::new()));
        let (d, l) = (data.clone(), logins.clone());
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    return;
                };
                let (d, l) = (d.clone(), l.clone());
                tokio::spawn(async move {
                    let _ = ftp_session(sock, d, l).await;
                });
            }
        });
        FtpServer {
            url: format!("ftp://127.0.0.1:{port}/pub/file%20one.bin"),
            data,
            sha256,
            logins,
        }
    }
}

async fn ftp_session(
    sock: TcpStream,
    data: Arc<Vec<u8>>,
    logins: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()> {
    let (r, mut w) = sock.into_split();
    let mut r = BufReader::new(r);
    w.write_all(b"220 test ftp\r\n").await?;
    let mut passive: Option<TcpListener> = None;
    let mut cwd = String::from("/");
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let line = line.trim_end();
        let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
        match cmd.to_ascii_uppercase().as_str() {
            "USER" => {
                logins.lock().unwrap().push(arg.to_string());
                w.write_all(b"331 password please\r\n").await?
            }
            "PASS" => w.write_all(b"230 logged in\r\n").await?,
            "TYPE" => w.write_all(b"200 type set\r\n").await?,
            "CWD" => {
                cwd = format!("{}{arg}/", cwd);
                w.write_all(b"250 ok\r\n").await?
            }
            "SIZE" => {
                if cwd == "/pub/" && arg == "file one.bin" {
                    w.write_all(format!("213 {}\r\n", data.len()).as_bytes())
                        .await?
                } else {
                    w.write_all(b"550 no such file\r\n").await?
                }
            }
            "PASV" => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let p = l.local_addr()?.port();
                passive = Some(l);
                w.write_all(
                    format!(
                        "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                        p / 256,
                        p % 256
                    )
                    .as_bytes(),
                )
                .await?
            }
            "EPSV" => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let p = l.local_addr()?.port();
                passive = Some(l);
                w.write_all(format!("229 Entering Extended Passive Mode (|||{p}|)\r\n").as_bytes())
                    .await?
            }
            "RETR" => {
                let Some(l) = passive.take() else {
                    w.write_all(b"425 use PASV first\r\n").await?;
                    continue;
                };
                if !(cwd == "/pub/" && arg == "file one.bin") {
                    w.write_all(b"550 no such file\r\n").await?;
                    continue;
                }
                w.write_all(b"150 opening data connection\r\n").await?;
                let (mut ds, _) = l.accept().await?;
                ds.write_all(&data).await?;
                ds.shutdown().await?;
                w.write_all(b"226 transfer complete\r\n").await?
            }
            "QUIT" => {
                w.write_all(b"221 bye\r\n").await?;
                return Ok(());
            }
            _ => w.write_all(b"502 not implemented\r\n").await?,
        }
    }
}
