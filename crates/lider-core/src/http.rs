//! Opening download streams over HTTP(S) and FTP (`HttpClient` in `engine.py`).
//!
//! What 1.x did through urllib, kept here on purpose:
//! - every request is a GET with `Accept: */*` and `Accept-Encoding: identity`, and an open-ended
//!   `Range: bytes=<pos>-` when ranged; extra headers from the download override those defaults;
//! - redirects are followed with every header kept (Range, Cookie, Authorization), up to 10;
//! - the timeout applies to the connect and to each read, not to the whole transfer;
//! - HTTP/1.1 only, so every connection of a download is its own TCP connection;
//! - with no proxy set, the `http_proxy`/`https_proxy`/`no_proxy` environment variables still apply.
//! - FTP is opened without headers or range, through no proxy, as one connection.

use std::error::Error as StdError;
use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine as _;
use bytes::Bytes;
use indexmap::IndexMap;
use regex::Regex;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::ftp::FtpBody;
use crate::models::Download;
use crate::names::{filename_from_url, parse_content_disposition, safe_filename};
use crate::pyutil::url_scheme;

/// The most redirects followed for one request, as urllib.
pub const MAX_REDIRECTS: usize = 10;

static CONTENT_RANGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^bytes\s+\d+-\d+/(\d+)").unwrap());

/// Why a request failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The server answered with an error status (4xx/5xx, or too many redirects).
    Status { code: u16, reason: String },
    /// Connecting or reading failed (refused, reset, timed out, bad TLS…).
    Network(String),
    /// The URL or the settings are not usable (bad URL, bad proxy, unsupported scheme).
    Invalid(String),
}

impl OpenError {
    /// The text 1.x shows for the error ("HTTP 404 Not Found", "timed out", …).
    pub fn message(&self) -> String {
        match self {
            OpenError::Status { code, reason } => format!("HTTP {code} {reason}"),
            OpenError::Network(m) | OpenError::Invalid(m) => m.clone(),
        }
    }
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for OpenError {}

/// The innermost cause of an error: "Connection refused (os error 111)" rather than reqwest's
/// "error sending request for url (…)".
pub fn describe(err: &(dyn StdError + 'static)) -> String {
    let mut inner: &(dyn StdError + 'static) = err;
    while let Some(next) = inner.source() {
        inner = next;
    }
    let text = inner.to_string();
    if text.is_empty() {
        err.to_string()
    } else {
        text
    }
}

fn timed_out() -> OpenError {
    OpenError::Network("timed out".into())
}

/// An open response body.
pub enum Body {
    Http(reqwest::Response),
    Ftp(Box<FtpBody>),
}

impl Body {
    /// The next piece of data that has arrived; `None` at the end.
    pub async fn chunk(&mut self, timeout: Duration) -> Result<Option<Bytes>, OpenError> {
        match self {
            Body::Http(resp) => match tokio::time::timeout(timeout, resp.chunk()).await {
                Err(_) => Err(timed_out()),
                Ok(Ok(c)) => Ok(c),
                Ok(Err(e)) => Err(OpenError::Network(describe(&e))),
            },
            Body::Ftp(f) => f.chunk(timeout).await,
        }
    }
}

/// An open response: status, headers and the final URL after redirects.
pub struct Response {
    /// HTTP status; FTP counts as 200.
    pub status: u16,
    pub url: String,
    pub headers: HeaderMap,
    /// `Content-Length` (FTP: the size the server reports), if any.
    pub length: Option<u64>,
    pub body: Body,
}

impl Response {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
}

/// What a first request tells about a download.
pub struct ProbeResult {
    /// The final URL after redirects.
    pub url: String,
    /// Total size, -1 when unknown.
    pub size: i64,
    /// The server answered the range request with 206 and a total size.
    pub resumable: bool,
    /// A safe file name from `Content-Disposition` or the URL.
    pub filename: String,
    pub content_type: String,
    /// The open response when the probe was asked to keep it: it becomes the first connection.
    pub response: Option<Response>,
}

/// Extra request headers of a download: its saved headers, then `Referer` and Basic authorization.
pub fn request_headers(d: &Download) -> IndexMap<String, String> {
    let mut h = d.headers.clone();
    if !d.referer.is_empty() {
        h.insert("Referer".into(), d.referer.clone());
    }
    if !d.username.is_empty() {
        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", d.username, d.password));
        h.insert("Authorization".into(), format!("Basic {token}"));
    }
    h
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    pub user_agent: String,
    pub timeout: Duration,
}

impl HttpClient {
    /// A client for one download. `proxy` "" = none set (environment proxies still apply).
    pub fn new(user_agent: &str, proxy: &str, timeout_secs: f64) -> Result<HttpClient, OpenError> {
        let timeout = Duration::from_secs_f64(if timeout_secs > 0.0 {
            timeout_secs
        } else {
            30.0
        });
        let mut b = reqwest::Client::builder()
            .http1_only()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeout)
            .pool_max_idle_per_host(0);
        if !proxy.trim().is_empty() {
            let p = reqwest::Proxy::all(proxy.trim())
                .map_err(|e| OpenError::Invalid(format!("Bad proxy address: {}", describe(&e))))?;
            b = b.proxy(p);
        }
        let client = b.build().map_err(|e| OpenError::Invalid(describe(&e)))?;
        Ok(HttpClient {
            client,
            user_agent: user_agent.to_string(),
            timeout,
        })
    }

    /// Open `url` at byte `start`. `ranged` asks for `bytes=<start>-`; FTP ignores both.
    pub async fn open(
        &self,
        url: &str,
        start: i64,
        ranged: bool,
        headers: &IndexMap<String, String>,
    ) -> Result<Response, OpenError> {
        if url_scheme(url) == "ftp" {
            return crate::ftp::open(url, self.timeout).await;
        }
        let mut h = HeaderMap::new();
        let mut set = |name: &str, value: &str| {
            if let (Ok(n), Ok(v)) = (
                HeaderName::from_bytes(name.trim().as_bytes()),
                HeaderValue::from_str(value),
            ) {
                h.insert(n, v);
            }
        };
        set("User-Agent", &self.user_agent);
        set("Accept", "*/*");
        set("Accept-Encoding", "identity");
        if ranged {
            set("Range", &format!("bytes={start}-"));
        }
        for (k, v) in headers {
            set(k, v);
        }
        let mut current = reqwest::Url::parse(url)
            .map_err(|e| OpenError::Invalid(format!("Not a valid address: {e}")))?;
        let mut last = StatusCode::FOUND;
        for _ in 0..=MAX_REDIRECTS {
            let send = self.client.get(current.clone()).headers(h.clone()).send();
            let resp = match tokio::time::timeout(self.timeout, send).await {
                Err(_) => return Err(timed_out()),
                Ok(Err(e)) => return Err(OpenError::Network(describe(&e))),
                Ok(Ok(r)) => r,
            };
            let status = resp.status();
            if status.is_redirection()
                && let Some(next) = resp
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|l| l.to_str().ok())
                    .and_then(|l| current.join(l).ok())
            {
                current = next;
                last = status;
                continue;
            }
            if status.is_client_error() || status.is_server_error() || status.is_redirection() {
                return Err(status_error(status));
            }
            let length = resp.content_length();
            return Ok(Response {
                status: status.as_u16(),
                url: resp.url().to_string(),
                headers: resp.headers().clone(),
                length,
                body: Body::Http(resp),
            });
        }
        // urllib's wording for a redirect loop.
        Err(OpenError::Status {
            code: last.as_u16(),
            reason: "The HTTP server returned a redirect error that would lead to an infinite loop"
                .into(),
        })
    }

    /// GET with `Range: bytes=0-` to learn size, resumability and the file name.
    pub async fn probe(
        &self,
        url: &str,
        headers: &IndexMap<String, String>,
        keep_open: bool,
    ) -> Result<ProbeResult, OpenError> {
        let resp = self.open(url, 0, true, headers).await?;
        let mut size = -1;
        let mut resumable = false;
        let total = CONTENT_RANGE
            .captures(resp.header("content-range"))
            .and_then(|m| m[1].parse::<i64>().ok());
        if let (206, Some(total)) = (resp.status, total) {
            size = total;
            resumable = true;
        } else if let Some(n) = resp.length {
            size = n as i64;
        }
        let name = parse_content_disposition(Some(resp.header("content-disposition")))
            .unwrap_or_else(|| filename_from_url(&resp.url));
        Ok(ProbeResult {
            url: resp.url.clone(),
            size,
            resumable,
            filename: safe_filename(&name),
            content_type: resp.header("content-type").to_string(),
            response: keep_open.then_some(resp),
        })
    }
}

fn status_error(status: StatusCode) -> OpenError {
    OpenError::Status {
        code: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_from_a_download() {
        let mut d = Download::new(1, "u", "f", "/d");
        d.headers.insert("Cookie".into(), "a=1".into());
        d.headers.insert("Referer".into(), "old".into());
        d.referer = "https://site/".into();
        d.username = "user".into();
        d.password = "pw".into();
        let h = request_headers(&d);
        assert_eq!(h["Cookie"], "a=1");
        assert_eq!(h["Referer"], "https://site/");
        assert_eq!(h["Authorization"], "Basic dXNlcjpwdw==");
        d.username.clear();
        assert!(!request_headers(&d).contains_key("Authorization"));
    }

    #[test]
    fn messages() {
        let e = OpenError::Status {
            code: 404,
            reason: "Not Found".into(),
        };
        assert_eq!(e.message(), "HTTP 404 Not Found");
        assert_eq!(timed_out().message(), "timed out");
    }

    #[test]
    fn bad_proxy_is_reported() {
        assert!(HttpClient::new("ua", "::not a proxy::", 5.0).is_err());
        assert!(HttpClient::new("ua", "", 5.0).is_ok());
        assert!(HttpClient::new("ua", "socks5://127.0.0.1:1080", 5.0).is_ok());
    }
}
