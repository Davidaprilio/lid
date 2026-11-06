//! Talking to a running LID: one abstract-namespace Unix socket per user (`ipc.py` and
//! `app.InstanceServer` in 1.x), with the same name and line protocol, so a 1.x and a 2.x LID and their
//! browser helpers understand each other.
//!
//! Each connection carries newline-separated lines. A plain line is an address to download (or
//! `--show`); a line starting with `{` is a JSON request from the browser extension.
//!
//! New in 2.x (decisions Q2, Q3): the server only takes connections from the same user (abstract sockets
//! have no file permissions), and a slow client no longer holds up the others or the UI.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// `LIDER_SOCKET_NAME`, or `lider-<uid>`; the leading NUL of the abstract namespace is added on use.
pub fn socket_name() -> String {
    std::env::var("LIDER_SOCKET_NAME")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("lider-{}", uid()))
}

pub fn uid() -> u32 {
    // SAFETY: getuid cannot fail.
    unsafe { libc::getuid() }
}

fn address(name: &str) -> std::io::Result<SocketAddr> {
    SocketAddr::from_abstract_name(name.as_bytes())
}

fn connect(name: &str) -> std::io::Result<UnixStream> {
    UnixStream::connect_addr(&address(name)?)
}

/// Hand lines to the running LID (`--show` when there are none). False when none is running.
pub fn send_lines(lines: &[String], name: &str) -> bool {
    let Ok(mut s) = connect(name) else {
        return false;
    };
    let text = if lines.is_empty() {
        "--show".to_string()
    } else {
        lines.join("\n")
    };
    s.write_all(format!("{text}\n").as_bytes()).is_ok()
}

/// Send one JSON request (compact, on one line).
pub fn send_request(request: &serde_json::Value, name: &str) -> bool {
    send_lines(&[request.to_string()], name)
}

pub fn is_running(name: &str) -> bool {
    connect(name).is_ok()
}

/// The user id at the other end of a Unix socket.
pub fn peer_uid(s: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: a valid socket fd and a buffer of the right size.
    let rc = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    (rc == 0).then_some(cred.uid)
}

/// The running instance's end: receives lines on a background thread and passes each non-empty,
/// trimmed line to `on_line`. Dropping it frees the name.
pub struct InstanceServer {
    listener: Arc<UnixListener>,
    closed: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl InstanceServer {
    /// Take the name; fails when another instance holds it.
    pub fn bind(
        name: &str,
        on_line: impl Fn(String) + Send + Sync + 'static,
    ) -> std::io::Result<InstanceServer> {
        let listener = Arc::new(UnixListener::bind_addr(&address(name)?)?);
        let closed = Arc::new(AtomicBool::new(false));
        let on_line = Arc::new(on_line);
        let (l, c) = (listener.clone(), closed.clone());
        let me = uid();
        let thread = thread::Builder::new()
            .name("lider-instance".into())
            .spawn(move || {
                for conn in l.incoming() {
                    if c.load(Ordering::Acquire) {
                        return;
                    }
                    let Ok(mut conn) = conn else { continue };
                    if peer_uid(&conn) != Some(me) {
                        continue; // another user on the machine
                    }
                    let on_line = on_line.clone();
                    let _ = thread::Builder::new()
                        .name("lider-instance-conn".into())
                        .spawn(move || {
                            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
                            let mut data = Vec::new();
                            let _ = conn.read_to_end(&mut data);
                            for line in String::from_utf8_lossy(&data).lines() {
                                let line = line.trim();
                                if !line.is_empty() {
                                    on_line(line.to_string());
                                }
                            }
                        });
                }
            })?;
        Ok(InstanceServer {
            listener,
            closed,
            thread: Some(thread),
        })
    }
}

impl Drop for InstanceServer {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        // SAFETY: shutting down our own listening socket wakes the blocked accept().
        unsafe {
            libc::shutdown(self.listener.as_raw_fd(), libc::SHUT_RDWR);
        }
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

    /// Port of test_app.py `SingleInstanceTests`.
    #[test]
    fn nobody_listening() {
        assert!(!send_lines(&["x".into()], &name("none")));
        assert!(!is_running(&name("none")));
    }

    #[test]
    fn lines_reach_the_running_instance() {
        let n = name("lines");
        let got = Arc::new(Mutex::new(Vec::<String>::new()));
        let g = got.clone();
        let server = InstanceServer::bind(&n, move |l| g.lock().unwrap().push(l)).unwrap();
        assert!(send_lines(
            &["https://a/1.zip".into(), "https://b/2.zip".into()],
            &n
        ));
        let end = Instant::now() + Duration::from_secs(5);
        while got.lock().unwrap().len() < 2 && Instant::now() < end {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(send_lines(&[], &n));
        while got.lock().unwrap().len() < 3 && Instant::now() < end {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            *got.lock().unwrap(),
            ["https://a/1.zip", "https://b/2.zip", "--show"]
        );
        assert!(is_running(&n));
        assert!(
            InstanceServer::bind(&n, |_| {}).is_err(),
            "a second instance cannot take the name"
        );
        drop(server);
        assert!(!send_lines(&["x".into()], &n));
    }

    #[test]
    fn json_request_is_one_compact_line() {
        let n = name("json");
        let got = Arc::new(Mutex::new(Vec::<String>::new()));
        let g = got.clone();
        let _server = InstanceServer::bind(&n, move |l| g.lock().unwrap().push(l)).unwrap();
        assert!(send_request(&serde_json::json!({"action": "show"}), &n));
        let end = Instant::now() + Duration::from_secs(5);
        while got.lock().unwrap().is_empty() && Instant::now() < end {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(*got.lock().unwrap(), [r#"{"action":"show"}"#]);
    }

    #[test]
    fn a_slow_client_does_not_block_others() {
        let n = name("slow");
        let got = Arc::new(Mutex::new(Vec::<String>::new()));
        let g = got.clone();
        let _server = InstanceServer::bind(&n, move |l| g.lock().unwrap().push(l)).unwrap();
        let _idle = connect(&n).unwrap(); // connects and says nothing
        let start = Instant::now();
        assert!(send_lines(&["https://c/3.zip".into()], &n));
        while got.lock().unwrap().is_empty() && start.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn peer_uid_is_ours() {
        let (a, _b) = UnixStream::pair().unwrap();
        assert_eq!(peer_uid(&a), Some(uid()));
    }
}
