//! Native messaging host for the LID browser extension (`native_host.py` in 1.x).
//!
//! The browser starts this program and talks to it over stdin/stdout: every message is a 32-bit
//! native-endian length followed by that many bytes of UTF-8 JSON. Requests are checked and handed to
//! the running LID (starting it when needed); the reply says whether LID took them.
//!
//! Requests:
//! - `{"type": "hello"}` -> version, capture switch, file types, excluded sites
//! - `{"type": "download", "url": ..., ...}` -> one address with referrer, cookies, user agent...
//! - `{"type": "links", "items": [...], ...}` -> a list of links for the "Download all links" dialog
//! - `{"type": "show"}` -> bring the LID window forward

use std::io::{self, Read, Write};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::platform::{Instance, Shell, os};
use crate::sh;

pub const MAX_MESSAGE: u32 = 8 * 1024 * 1024;
pub const MAX_LINKS: usize = 5000;
/// Text fields of a download request and the most characters kept of each.
pub const TEXT_FIELDS: [(&str, usize); 8] = [
    ("url", 8192),
    ("final_url", 8192),
    ("referer", 8192),
    ("page_url", 8192),
    ("filename", 1024),
    ("mime", 256),
    ("cookies", 65536),
    ("user_agent", 1024),
];

/// `^(https?|ftp)://\S+$` as Python matches it (`$` also before a final newline).
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(https?|ftp)://\S+\n?\z").unwrap());

pub fn is_url(s: &str) -> bool {
    URL_RE.is_match(s)
}

fn limit(field: &str) -> usize {
    TEXT_FIELDS
        .iter()
        .find(|(k, _)| *k == field)
        .map_or(0, |(_, n)| *n)
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Debug, PartialEq)]
pub enum FrameError {
    TooLarge(u32),
    NotUtf8,
    NotJson,
    NotObject,
    Io(String),
}

impl FrameError {
    /// The text sent back in `{"ok": false, "error": …}` before the host exits.
    pub fn message(&self) -> String {
        match self {
            FrameError::TooLarge(n) => format!("message of {n} bytes is too large"),
            FrameError::NotUtf8 => "the message is not valid UTF-8".into(),
            FrameError::NotJson => "the message is not valid JSON".into(),
            FrameError::NotObject => "a message must be a JSON object".into(),
            FrameError::Io(e) => e.clone(),
        }
    }
}

/// The next message; `Ok(None)` at the end of the input (or a short read).
pub fn read_message(r: &mut impl Read) -> Result<Option<Map<String, Value>>, FrameError> {
    let mut head = [0u8; 4];
    if !read_full(r, &mut head)? {
        return Ok(None);
    }
    let size = u32::from_ne_bytes(head);
    if size > MAX_MESSAGE {
        return Err(FrameError::TooLarge(size));
    }
    let mut body = vec![0u8; size as usize];
    if !read_full(r, &mut body)? {
        return Ok(None);
    }
    let text = String::from_utf8(body).map_err(|_| FrameError::NotUtf8)?;
    match serde_json::from_str::<Value>(&text).map_err(|_| FrameError::NotJson)? {
        Value::Object(m) => Ok(Some(m)),
        _ => Err(FrameError::NotObject),
    }
}

/// Fill `buf`; false when the input ends first.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> Result<bool, FrameError> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => return Ok(false),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(FrameError::Io(e.to_string())),
        }
    }
    Ok(true)
}

pub fn write_message(w: &mut impl Write, obj: &Value) -> io::Result<()> {
    let data = obj.to_string();
    w.write_all(&(data.len() as u32).to_ne_bytes())?;
    w.write_all(data.as_bytes())?;
    w.flush()
}

/// A download request with only the known fields, cut to their limits; `None` without a usable URL.
pub fn clean_download(msg: &Map<String, Value>) -> Option<Value> {
    let mut req = Map::new();
    req.insert("action".into(), "download".into());
    for (key, n) in TEXT_FIELDS {
        if let Some(Value::String(v)) = msg.get(key)
            && !v.is_empty()
        {
            req.insert(key.into(), truncate(v, n).into());
        }
    }
    // An integer above 0; a JSON true is not a size (decision Q5).
    if let Some(n) = msg.get("size").and_then(Value::as_i64).filter(|n| *n > 0) {
        req.insert("size".into(), n.into());
    }
    if !req.get("url").and_then(Value::as_str).is_some_and(is_url) {
        return None;
    }
    for key in ["final_url", "referer", "page_url"] {
        if req
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|v| !is_url(v))
        {
            req.remove(key);
        }
    }
    Some(Value::Object(req))
}

/// A links request with the usable links only (at most 5000); `None` when no link is left.
pub fn clean_links(msg: &Map<String, Value>) -> Option<Value> {
    let empty = vec![];
    let items: Vec<Value> = msg
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .take(MAX_LINKS)
        .filter_map(|item| {
            let item = item.as_object()?;
            let url = item.get("url")?.as_str()?;
            if !is_url(url) || url.chars().count() > 8192 {
                return None;
            }
            let text = item.get("text").and_then(Value::as_str).unwrap_or("");
            let kind = item
                .get("kind")
                .and_then(Value::as_str)
                .filter(|k| ["a", "img", "video", "audio"].contains(k))
                .unwrap_or("a");
            Some(json!({"url": url, "text": truncate(text, 300), "kind": kind}))
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    let mut req = Map::new();
    req.insert("action".into(), "links".into());
    req.insert("items".into(), Value::Array(items));
    for key in ["referer", "page_url", "cookies", "user_agent"] {
        if let Some(Value::String(v)) = msg.get(key)
            && !v.is_empty()
        {
            // The page addresses must be web addresses too (decision Q5).
            if (key == "referer" || key == "page_url") && !is_url(v) {
                continue;
            }
            req.insert(key.into(), truncate(v, limit(key)).into());
        }
    }
    Some(Value::Object(req))
}

/// How to start LID: `LIDER_LAUNCH` from the wrapper script, else this program with `--minimized`.
pub fn launch_command() -> Vec<String> {
    if let Some(cmd) = std::env::var("LIDER_LAUNCH").ok().filter(|v| !v.is_empty())
        && let Some(args) = sh::split(&cmd)
        && !args.is_empty()
    {
        return args;
    }
    let exe = std::env::current_exe()
        .map_or_else(|_| "lider".into(), |p| p.to_string_lossy().into_owned());
    vec![exe, "--minimized".into()]
}

/// Start a program detached from the browser (own session, no stdio).
fn spawn_detached(cmd: &[String]) -> bool {
    cmd.split_first()
        .is_some_and(|(program, args)| os().spawn_detached(program, args).is_ok())
}

/// Send to the running LID; start it and retry for up to `wait` when it is not running.
pub fn deliver(
    request: &Value,
    socket: &str,
    launch: Option<&dyn Fn() -> bool>,
    wait: Duration,
) -> bool {
    if os().send_request(request, socket) {
        return true;
    }
    let started = match launch {
        Some(f) => f(),
        None => spawn_detached(&launch_command()),
    };
    if !started {
        return false;
    }
    let end = Instant::now() + wait;
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(250));
        if os().send_request(request, socket) {
            return true;
        }
    }
    false
}

/// The reply to one message.
pub fn handle(
    msg: &Map<String, Value>,
    socket: &str,
    dirs: &lider_core::paths::Dirs,
    launch: Option<&dyn Fn() -> bool>,
) -> Value {
    let kind = msg.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "hello" => {
            let s = lider_core::settings::load(dirs);
            json!({
                "ok": true,
                "version": lider_core::VERSION,
                "capture": s.browser_capture,
                "types": s.auto_capture_types.split_whitespace().collect::<Vec<_>>(),
                "skip_sites": s.no_auto_sites.split_whitespace().collect::<Vec<_>>(),
                "running": os().is_running(socket),
            })
        }
        "download" | "links" | "show" => {
            let request = match kind {
                "show" => Some(json!({"action": "show"})),
                "download" => clean_download(msg),
                _ => clean_links(msg),
            };
            let Some(request) = request else {
                return json!({"ok": false, "error": "invalid request"});
            };
            if deliver(&request, socket, launch, Duration::from_secs(15)) {
                json!({"ok": true})
            } else {
                json!({"ok": false, "error": "LID is not running and could not be started"})
            }
        }
        // A fixed text instead of Python's repr of the value (decision Q6).
        _ => json!({"ok": false, "error": "unknown message type"}),
    }
}

/// `lider --native-host`: answer messages until the browser closes stdin. Exit code 0 at the end of the
/// input, 1 after a framing error (which is answered first).
pub fn main() -> i32 {
    let dirs = lider_core::paths::Dirs::from_env();
    let socket = os().instance_name();
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    loop {
        match read_message(&mut stdin) {
            Ok(None) => return 0,
            Ok(Some(msg)) => {
                if write_message(&mut stdout, &handle(&msg, &socket, &dirs, None)).is_err() {
                    return 1;
                }
            }
            Err(e) => {
                let _ = write_message(&mut stdout, &json!({"ok": false, "error": e.message()}));
                return 1;
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener};
    use std::sync::{Arc, Mutex};

    pub fn frame(v: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        write_message(&mut out, v).unwrap();
        out
    }

    /// Collects the lines a running LID would receive on its socket.
    struct FakeLid {
        lines: Arc<Mutex<Vec<String>>>,
    }

    impl FakeLid {
        fn start(name: &str) -> FakeLid {
            let l =
                UnixListener::bind_addr(&SocketAddr::from_abstract_name(name.as_bytes()).unwrap())
                    .unwrap();
            let lines = Arc::new(Mutex::new(Vec::new()));
            let got = lines.clone();
            std::thread::spawn(move || {
                for c in l.incoming() {
                    let Ok(mut c) = c else { return };
                    let mut s = String::new();
                    let _ = c.read_to_string(&mut s);
                    got.lock().unwrap().extend(s.lines().map(String::from));
                }
            });
            FakeLid { lines }
        }

        fn requests(&self) -> Vec<Value> {
            let end = Instant::now() + Duration::from_secs(3);
            while self.lines.lock().unwrap().is_empty() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(10));
            }
            self.lines
                .lock()
                .unwrap()
                .iter()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        }
    }

    fn name(tag: &str) -> String {
        format!("lider-test-host-{tag}-{}", std::process::id())
    }

    fn msg(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn dirs(tmp: &std::path::Path) -> lider_core::paths::Dirs {
        lider_core::paths::Dirs::from_parts(tmp.into(), Some(tmp.join("c").into()), None, None)
    }

    #[test]
    fn round_trip() {
        let bytes = frame(&json!({"type": "hello", "ü": 1}));
        assert_eq!(
            read_message(&mut Cursor::new(bytes)).unwrap(),
            Some(msg(json!({"type": "hello", "ü": 1})))
        );
    }

    #[test]
    fn end_of_input() {
        assert_eq!(read_message(&mut Cursor::new(Vec::new())).unwrap(), None);
        assert_eq!(
            read_message(&mut Cursor::new(vec![5, 0, 0, 0, b'{'])).unwrap(),
            None
        );
    }

    #[test]
    fn oversized_message_is_refused() {
        let head = (64u32 * 1024 * 1024).to_ne_bytes().to_vec();
        assert_eq!(
            read_message(&mut Cursor::new(head)),
            Err(FrameError::TooLarge(64 * 1024 * 1024))
        );
        let mut bad = 3u32.to_ne_bytes().to_vec();
        bad.extend(b"[1]");
        assert_eq!(
            read_message(&mut Cursor::new(bad)),
            Err(FrameError::NotObject)
        );
    }

    #[test]
    fn hello_reports_the_capture_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let d = dirs(tmp.path());
        let mut s = lider_core::settings::Settings::defaults(&d);
        s.browser_capture = false;
        s.auto_capture_types = "ZIP ISO".into();
        s.no_auto_sites = "example.com".into();
        lider_core::settings::save(&s, &d).unwrap();
        let r = handle(&msg(json!({"type": "hello"})), &name("hello"), &d, None);
        assert_eq!(
            (
                &r["ok"],
                &r["capture"],
                &r["types"],
                &r["skip_sites"],
                &r["running"]
            ),
            (
                &json!(true),
                &json!(false),
                &json!(["ZIP", "ISO"]),
                &json!(["example.com"]),
                &json!(false)
            )
        );
        assert_eq!(r["version"], lider_core::VERSION);
    }

    #[test]
    fn download_is_forwarded_with_only_known_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let n = name("dl");
        let lid = FakeLid::start(&n);
        let r = handle(
            &msg(
                json!({"type": "download", "url": "https://x.org/a.zip", "referer": "https://x.org/",
                        "cookies": "s=1", "user_agent": "UA", "size": 10, "evil": "x",
                        "page_url": "javascript:alert(1)"}),
            ),
            &n,
            &dirs(tmp.path()),
            None,
        );
        assert_eq!(r, json!({"ok": true}));
        assert_eq!(
            lid.requests(),
            [
                json!({"action": "download", "url": "https://x.org/a.zip", "referer": "https://x.org/",
                    "cookies": "s=1", "user_agent": "UA", "size": 10})
            ]
        );
    }

    #[test]
    fn bad_urls_are_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let d = dirs(tmp.path());
        let never: &dyn Fn() -> bool = &|| false;
        for url in [
            json!("javascript:alert(1)"),
            json!("file:///etc/passwd"),
            json!(""),
            Value::Null,
        ] {
            let r = handle(
                &msg(json!({"type": "download", "url": url})),
                &name("bad"),
                &d,
                Some(never),
            );
            assert_eq!(r["ok"], false);
        }
        let r = handle(
            &msg(json!({"type": "links", "items": [{"url": "data:text/html,x"}]})),
            &name("bad"),
            &d,
            Some(never),
        );
        assert_eq!(r["error"], "invalid request");
    }

    #[test]
    fn size_must_be_a_positive_integer() {
        let base = |size: Value| {
            clean_download(&msg(json!({"url": "https://x/a", "size": size}))).unwrap()
        };
        assert_eq!(base(json!(10))["size"], 10);
        for bad in [json!(true), json!(0), json!(-3), json!(1.5), json!("10")] {
            assert!(base(bad).get("size").is_none());
        }
    }

    #[test]
    fn long_fields_are_cut() {
        let r = clean_download(&msg(
            json!({"url": "https://x/a", "filename": "é".repeat(2000)}),
        ))
        .unwrap();
        assert_eq!(r["filename"].as_str().unwrap().chars().count(), 1024);
    }

    #[test]
    fn links_are_cleaned() {
        let tmp = tempfile::tempdir().unwrap();
        let n = name("links");
        let lid = FakeLid::start(&n);
        let r = handle(
            &msg(
                json!({"type": "links", "page_url": "https://x.org/p", "referer": "javascript:x", "items": [
                {"url": "https://x.org/a.zip", "text": "A", "kind": "a"},
                {"url": "ftp://x.org/b.iso", "kind": "weird"},
                {"url": "chrome://settings"}, "not a dict"]}),
            ),
            &n,
            &dirs(tmp.path()),
            None,
        );
        assert_eq!(r["ok"], true);
        let req = &lid.requests()[0];
        assert_eq!(
            req["items"],
            json!([{"url": "https://x.org/a.zip", "text": "A", "kind": "a"},
                   {"url": "ftp://x.org/b.iso", "text": "", "kind": "a"}])
        );
        assert_eq!(req["page_url"], "https://x.org/p");
        assert!(req.get("referer").is_none());
    }

    #[test]
    fn starts_lid_when_it_is_not_running() {
        let n = name("start");
        let started: Arc<Mutex<Option<FakeLid>>> = Arc::new(Mutex::new(None));
        let s = started.clone();
        let n2 = n.clone();
        let launch = move || {
            *s.lock().unwrap() = Some(FakeLid::start(&n2));
            true
        };
        assert!(deliver(
            &json!({"action": "show"}),
            &n,
            Some(&launch),
            Duration::from_secs(5)
        ));
        assert_eq!(
            started.lock().unwrap().as_ref().unwrap().requests(),
            [json!({"action": "show"})]
        );
    }

    #[test]
    fn not_started_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let never: &dyn Fn() -> bool = &|| false;
        let r = handle(
            &msg(json!({"type": "show"})),
            &name("nostart"),
            &dirs(tmp.path()),
            Some(never),
        );
        assert_eq!(
            r,
            json!({"ok": false, "error": "LID is not running and could not be started"})
        );
    }

    #[test]
    fn unknown_type() {
        let tmp = tempfile::tempdir().unwrap();
        let r = handle(
            &msg(json!({"type": "format-disk"})),
            &name("x"),
            &dirs(tmp.path()),
            None,
        );
        assert_eq!(r, json!({"ok": false, "error": "unknown message type"}));
    }

    #[test]
    fn url_rule_like_python() {
        assert!(is_url("https://x.org/a.zip"));
        assert!(is_url("FTP://x/y"));
        assert!(is_url("https://x/y\n"));
        assert!(!is_url("https://x/a b"));
        assert!(!is_url("https://"));
        assert!(!is_url("file:///etc/passwd"));
    }
}
