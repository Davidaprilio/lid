//! Optional checksum of a download: read what the user pastes, hash the finished file
//! (`checksum.py` in 1.x). The error texts are shown under the field and are kept word for word.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use md5::Md5;
use regex::Regex;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

/// Algorithm name and hex length, in the order the UI lists them.
pub const ALGORITHMS: [(&str, usize); 4] =
    [("md5", 32), ("sha1", 40), ("sha256", 64), ("sha512", 128)];

/// "SHA-256" for "sha256".
pub fn algorithm_title(name: &str) -> &'static str {
    match name {
        "md5" => "MD5",
        "sha1" => "SHA-1",
        "sha256" => "SHA-256",
        "sha512" => "SHA-512",
        _ => "",
    }
}

fn hex_len(name: &str) -> Option<usize> {
    ALGORITHMS.iter().find(|(a, _)| *a == name).map(|(_, n)| *n)
}

fn by_length(len: usize) -> Option<&'static str> {
    ALGORITHMS.iter().find(|(_, n)| *n == len).map(|(a, _)| *a)
}

static PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(md5|sha-?1|sha-?256|sha-?512)\s*[:=]\s*").unwrap());

/// The text is not a checksum; the message says why, for the field's error line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ChecksumError(pub String);

/// `"<algorithm>:<lower-case hex>"` for `text`, `""` for an empty field.
///
/// Accepts plain hex, a `sha256:<hex>` style prefix, and a line of a SHA256SUMS file (`<hex>  name`
/// or `<hex> *name`, the name is dropped). `algorithm` "auto" picks it from the prefix or the length;
/// any other value forces it and the length has to match.
pub fn parse(text: &str, algorithm: &str) -> Result<String, ChecksumError> {
    let mut s = text.trim();
    if s.is_empty() {
        return Ok(String::new());
    }
    let mut named: Option<String> = None;
    if let Some(m) = PREFIX.captures(s) {
        named = Some(m[1].to_lowercase().replace('-', ""));
        s = s[m.get(0).unwrap().end()..].trim();
    }
    let value = s
        .split_whitespace()
        .next()
        .map(str::to_lowercase)
        .unwrap_or_default();
    if value.is_empty() {
        return Err(ChecksumError(
            "Not a valid checksum — there is no value after the prefix.".into(),
        ));
    }
    if let Some(bad) = value
        .chars()
        .find(|c| !c.is_ascii_hexdigit() || c.is_ascii_uppercase())
    {
        return Err(ChecksumError(format!(
            "Not a valid checksum — “{bad}” is not a hex character."
        )));
    }
    if let Some(n) = &named
        && algorithm != "auto"
        && algorithm != n
    {
        return Err(ChecksumError(format!(
            "The value is marked {} but {} is picked.",
            algorithm_title(n),
            algorithm_title(algorithm)
        )));
    }
    let algo = match named {
        Some(n) => Some(n),
        None if algorithm != "auto" => Some(algorithm.to_string()),
        None => by_length(value.chars().count()).map(String::from),
    };
    let len = value.chars().count();
    let Some(algo) = algo else {
        return Err(ChecksumError(format!(
            "Not a valid checksum — expected 32, 40, 64 or 128 hex characters, got {len}."
        )));
    };
    let want = hex_len(&algo).unwrap_or(0);
    if len != want {
        return Err(ChecksumError(format!(
            "Not a valid {} value — expected {want} hex characters, got {len}.",
            algorithm_title(&algo)
        )));
    }
    Ok(format!("{algo}:{value}"))
}

/// `("sha256", "<hex>")` from a stored `"sha256:<hex>"`.
pub fn split(checksum: &str) -> (&str, &str) {
    checksum.split_once(':').unwrap_or((checksum, ""))
}

/// "SHA-256" for a stored checksum, "" for none.
pub fn title(checksum: &str) -> &'static str {
    if checksum.is_empty() {
        ""
    } else {
        algorithm_title(split(checksum).0)
    }
}

/// What a check ended with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    Verified,
    Mismatch,
    /// There was no value to compare with; `actual` holds the hash.
    Computed,
    Skipped,
    Error,
}

impl CheckResult {
    /// The value stored in `Download::checksum_result`.
    pub fn as_str(self) -> &'static str {
        match self {
            CheckResult::Verified => "verified",
            CheckResult::Mismatch => "mismatch",
            CheckResult::Computed => "computed",
            CheckResult::Skipped => "skipped",
            CheckResult::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub result: CheckResult,
    pub actual: String,
    pub error: String,
    pub seconds: f64,
}

/// Hashes a finished file in 1 MiB reads. Run `run()` on a worker thread; the UI side reads
/// `progress()` and may call `skip()`. The manager copies the outcome to the download.
pub struct Verifier {
    pub path: PathBuf,
    pub algorithm: String,
    pub expected: String,
    /// "Verify now" in Properties: no Complete dialog afterwards.
    pub manual: bool,
    pub download_id: i64,
    read: AtomicU64,
    total: AtomicU64,
    skip: AtomicBool,
    running: AtomicBool,
    outcome: Mutex<Option<Outcome>>,
}

const CHUNK: usize = 1024 * 1024;

enum Hasher {
    Md5(Md5),
    Sha1(Sha1),
    Sha256(Sha256),
    Sha512(Sha512),
}

impl Hasher {
    fn new(name: &str) -> Option<Hasher> {
        Some(match name {
            "md5" => Hasher::Md5(Md5::new()),
            "sha1" => Hasher::Sha1(Sha1::new()),
            "sha256" => Hasher::Sha256(Sha256::new()),
            "sha512" => Hasher::Sha512(Sha512::new()),
            _ => return None,
        })
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Md5(h) => h.update(data),
            Hasher::Sha1(h) => h.update(data),
            Hasher::Sha256(h) => h.update(data),
            Hasher::Sha512(h) => h.update(data),
        }
    }

    fn hex(self) -> String {
        match self {
            Hasher::Md5(h) => format!("{:x}", h.finalize()),
            Hasher::Sha1(h) => format!("{:x}", h.finalize()),
            Hasher::Sha256(h) => format!("{:x}", h.finalize()),
            Hasher::Sha512(h) => format!("{:x}", h.finalize()),
        }
    }
}

impl Verifier {
    pub fn new(
        path: impl Into<PathBuf>,
        checksum: &str,
        manual: bool,
        download_id: i64,
    ) -> Verifier {
        let (algorithm, expected) = split(checksum);
        Verifier {
            path: path.into(),
            algorithm: algorithm.into(),
            expected: expected.into(),
            manual,
            download_id,
            read: AtomicU64::new(0),
            total: AtomicU64::new(0),
            skip: AtomicBool::new(false),
            running: AtomicBool::new(true),
            outcome: Mutex::new(None),
        }
    }

    /// 0..1 of the file read so far.
    pub fn progress(&self) -> f64 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (self.read.load(Ordering::Relaxed) as f64 / total as f64).min(1.0)
        }
    }

    pub fn skip(&self) {
        self.skip.store(true, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// The result once `run()` has finished.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Hash the file (blocking) and store the outcome.
    pub fn run(&self) -> Outcome {
        let started = Instant::now();
        let (result, actual, error) = match self.hash() {
            Ok(None) => (CheckResult::Skipped, String::new(), String::new()),
            Ok(Some(actual)) => {
                let result = if self.expected.is_empty() {
                    CheckResult::Computed
                } else if actual == self.expected {
                    CheckResult::Verified
                } else {
                    CheckResult::Mismatch
                };
                (result, actual, String::new())
            }
            Err(e) => (CheckResult::Error, String::new(), e),
        };
        let outcome = Outcome {
            result,
            actual,
            error,
            seconds: started.elapsed().as_secs_f64(),
        };
        *self.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome.clone());
        self.running.store(false, Ordering::Release);
        outcome
    }

    fn hash(&self) -> Result<Option<String>, String> {
        let mut h = Hasher::new(&self.algorithm)
            .ok_or_else(|| format!("unsupported hash type {}", self.algorithm))?;
        let describe = |e: std::io::Error| format!("{e}: '{}'", self.path.display());
        let mut f = File::open(&self.path).map_err(describe)?;
        let total = f.metadata().map_err(describe)?.len();
        self.total.store(total, Ordering::Relaxed);
        let mut buf = vec![0u8; CHUNK];
        while !self.skip.load(Ordering::Relaxed) {
            let n = f.read(&mut buf).map_err(describe)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            self.read.fetch_add(n as u64, Ordering::Relaxed);
        }
        if self.skip.load(Ordering::Relaxed) {
            return Ok(None);
        }
        Ok(Some(h.hex()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "3f9c1e7a5b2d48c6e0a1f7b39d4c2e85a6b0f13d7e9c4a2b58f6d01e3c7a9b42";

    /// Port of test_manager.py `test_accepted_input`.
    #[test]
    fn accepted_input() {
        assert_eq!(parse("", "auto").unwrap(), "");
        let sha = format!("sha256:{H}");
        assert_eq!(
            parse(&format!("  {}\n", H.to_uppercase()), "auto").unwrap(),
            sha
        );
        assert_eq!(parse(&format!("SHA-256: {H}"), "auto").unwrap(), sha);
        assert_eq!(parse(&format!("{H} *ubuntu.iso"), "auto").unwrap(), sha);
        assert_eq!(parse(&format!("{H}  ubuntu.iso"), "auto").unwrap(), sha);
        assert_eq!(
            parse("d41d8cd98f00b204e9800998ecf8427e", "auto").unwrap(),
            "md5:d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(parse(H, "sha256").unwrap(), sha);
        assert_eq!(
            parse(&format!("sha1={}", "a".repeat(40)), "auto").unwrap(),
            format!("sha1:{}", "a".repeat(40))
        );
    }

    /// Port of test_manager.py `test_invalid_input_says_why`.
    #[test]
    fn invalid_input_says_why() {
        let short = &H[..H.len() - 2];
        assert_eq!(
            parse(short, "sha256").unwrap_err().0,
            "Not a valid SHA-256 value — expected 64 hex characters, got 62."
        );
        assert_eq!(
            parse(short, "auto").unwrap_err().0,
            "Not a valid checksum — expected 32, 40, 64 or 128 hex characters, got 62."
        );
        assert_eq!(
            parse(&format!("g{}", &H[1..]), "auto").unwrap_err().0,
            "Not a valid checksum — “g” is not a hex character."
        );
        assert_eq!(
            parse(&format!("G{}", &H[1..]), "auto").unwrap_err().0,
            "Not a valid checksum — “g” is not a hex character."
        );
        assert_eq!(
            parse(&format!("sha256:{H}"), "md5").unwrap_err().0,
            "The value is marked SHA-256 but MD5 is picked."
        );
        assert_eq!(
            parse("sha256:", "auto").unwrap_err().0,
            "Not a valid checksum — there is no value after the prefix."
        );
    }

    #[test]
    fn split_and_title() {
        assert_eq!(split("sha256:ab"), ("sha256", "ab"));
        assert_eq!(split("md5:"), ("md5", ""));
        assert_eq!(title("sha512:00"), "SHA-512");
        assert_eq!(title(""), "");
    }

    /// Port of test_manager.py `test_verifier_computes_without_an_expected_value`.
    #[test]
    fn verifier_computes_without_an_expected_value() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "lider".repeat(1000)).unwrap();
        let v = Verifier::new(tmp.path(), "md5:", false, 1);
        let out = v.run();
        assert_eq!(out.result, CheckResult::Computed);
        assert_eq!(
            out.actual,
            format!("{:x}", Md5::digest("lider".repeat(1000)))
        );
        assert!(!v.is_running());
        assert_eq!(v.progress(), 1.0);
    }

    #[test]
    fn verifier_results() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"abc").unwrap();
        let good = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(
            Verifier::new(tmp.path(), good, false, 1).run().result,
            CheckResult::Verified
        );
        let bad = format!("sha256:{}", "0".repeat(64));
        assert_eq!(
            Verifier::new(tmp.path(), &bad, false, 1).run().result,
            CheckResult::Mismatch
        );
        let sha1 = Verifier::new(tmp.path(), "sha1:", false, 1).run();
        assert_eq!(sha1.actual, "a9993e364706816aba3e25717850c26c9cd0d89d");
        let sha512 = Verifier::new(tmp.path(), "sha512:", false, 1).run();
        assert!(sha512.actual.starts_with("ddaf35a193617aba"));
        let skipped = Verifier::new(tmp.path(), good, false, 1);
        skipped.skip();
        assert_eq!(skipped.run().result, CheckResult::Skipped);
        let missing = Verifier::new("/nonexistent/x", good, false, 1).run();
        assert_eq!(missing.result, CheckResult::Error);
        assert!(missing.error.contains("/nonexistent/x"));
        assert_eq!(
            Verifier::new(tmp.path(), "", false, 1).run().result,
            CheckResult::Error
        );
    }
}
