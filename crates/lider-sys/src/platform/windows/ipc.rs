//! The single instance on Windows: a named pipe `\\.\pipe\<name>` with the same line protocol as the
//! Linux socket. The server is tokio's named pipe (first instance only, local clients only); the default
//! security of a pipe lets other users read but not write, so only this user can hand LID lines.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};

/// `ERROR_PIPE_BUSY`: every instance is taken; the server is there.
const PIPE_BUSY: i32 = 231;

/// `LIDER_SOCKET_NAME`, or `lider-<user>`.
pub fn pipe_name() -> String {
    std::env::var("LIDER_SOCKET_NAME")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
            format!("lider-{}", user.replace(['\\', '/'], "_"))
        })
}

fn path(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

/// Open the client end, waiting a little while the server sets up its next instance.
fn connect(name: &str) -> io::Result<std::fs::File> {
    let mut last = io::Error::from(io::ErrorKind::NotFound);
    for _ in 0..20 {
        match OpenOptions::new().write(true).open(path(name)) {
            Ok(f) => return Ok(f),
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) => {
                last = e;
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

pub fn send_lines(lines: &[String], name: &str) -> bool {
    let Ok(mut f) = connect(name) else {
        return false;
    };
    let text = if lines.is_empty() {
        "--show".to_string()
    } else {
        lines.join("\n")
    };
    f.write_all(format!("{text}\n").as_bytes()).is_ok()
}

pub fn is_running(name: &str) -> bool {
    match OpenOptions::new().write(true).open(path(name)) {
        Ok(_) => true,
        Err(e) => e.raw_os_error() == Some(PIPE_BUSY),
    }
}

/// The running LID's end: every connection's lines go to `on_line`. Dropping it frees the name.
pub struct InstanceServer {
    closed: Arc<AtomicBool>,
    name: String,
    thread: Option<thread::JoinHandle<()>>,
}

impl InstanceServer {
    pub fn bind(
        name: &str,
        on_line: Box<dyn Fn(String) + Send + Sync>,
    ) -> io::Result<InstanceServer> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let pipe = path(name);
        let options = || {
            let mut o = ServerOptions::new();
            o.access_inbound(true)
                .access_outbound(false)
                .pipe_mode(PipeMode::Byte)
                .reject_remote_clients(true);
            o
        };
        // Fails when another LID already has the name.
        let first = rt.block_on(async { options().first_pipe_instance(true).create(&pipe) })?;
        let closed = Arc::new(AtomicBool::new(false));
        let c = closed.clone();
        let on_line: Arc<dyn Fn(String) + Send + Sync> = Arc::from(on_line);
        let thread = thread::Builder::new()
            .name("lider-instance".into())
            .spawn(move || {
                rt.block_on(async move {
                    let mut server = first;
                    loop {
                        let connected = server.connect().await.is_ok();
                        if c.load(Ordering::Acquire) {
                            return;
                        }
                        // The next instance exists before this one is handed off, so the name stays taken.
                        let Ok(next) = options().create(&pipe) else {
                            return;
                        };
                        let mut conn = std::mem::replace(&mut server, next);
                        if !connected {
                            continue;
                        }
                        let on_line = on_line.clone();
                        tokio::spawn(async move {
                            let mut data = Vec::new();
                            let read = tokio::time::timeout(
                                Duration::from_secs(2),
                                conn.read_to_end(&mut data),
                            );
                            let _ = read.await;
                            for line in String::from_utf8_lossy(&data).lines() {
                                let line = line.trim();
                                if !line.is_empty() {
                                    on_line(line.to_string());
                                }
                            }
                        });
                    }
                })
            })?;
        Ok(InstanceServer {
            closed,
            name: name.to_string(),
            thread: Some(thread),
        })
    }
}

impl Drop for InstanceServer {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        // A last connection wakes the waiting connect().
        let _ = OpenOptions::new().write(true).open(path(&self.name));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;

    fn name(tag: &str) -> String {
        format!("lider-test-{tag}-{}", std::process::id())
    }

    #[test]
    fn lines_reach_the_running_instance() {
        let n = name("lines");
        assert!(!is_running(&n));
        assert!(!send_lines(&["x".into()], &n));
        let got = Arc::new(Mutex::new(Vec::<String>::new()));
        let g = got.clone();
        let server =
            InstanceServer::bind(&n, Box::new(move |l| g.lock().unwrap().push(l))).unwrap();
        assert!(is_running(&n));
        assert!(send_lines(
            &["https://a/1.zip".into(), "https://b/2.zip".into()],
            &n
        ));
        assert!(send_lines(&[], &n));
        let end = Instant::now() + Duration::from_secs(5);
        while got.lock().unwrap().len() < 3 && Instant::now() < end {
            thread::sleep(Duration::from_millis(10));
        }
        let mut lines = got.lock().unwrap().clone();
        lines.sort();
        assert_eq!(lines, ["--show", "https://a/1.zip", "https://b/2.zip"]);
        assert!(
            InstanceServer::bind(&n, Box::new(|_| {})).is_err(),
            "a second LID cannot take the name"
        );
        drop(server);
        assert!(!is_running(&n));
    }
}
