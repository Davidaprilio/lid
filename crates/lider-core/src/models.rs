//! Download records shared by the engine, the store and the UI (`models.py` in 1.x).
//!
//! The JSON form is the one 1.x stores in `downloads.db`, field for field. Fields that this version
//! does not know are kept in `extra` and written back, so going back to an older or newer LID does not
//! lose them.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const MAIN_QUEUE: &str = "Main download queue";
pub const SYNC_QUEUE: &str = "Synchronization queue";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Added but never started; the list shows an empty status.
    #[default]
    Queued,
    Connecting,
    Downloading,
    /// Stopped by the user; the list shows the percentage.
    Paused,
    Assembling,
    /// The file is complete on disk and its checksum is being read.
    Verifying,
    Complete,
    Error,
}

impl Status {
    /// Connecting, downloading or assembling: work is in progress.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Status::Connecting | Status::Downloading | Status::Assembling
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Connecting => "connecting",
            Status::Downloading => "downloading",
            Status::Paused => "paused",
            Status::Assembling => "assembling",
            Status::Verifying => "verifying",
            Status::Complete => "complete",
            Status::Error => "error",
        }
    }
}

/// Byte range `[start, end)` of the file; `pos` is the next byte to write.
/// `end` is -1 while the size is unknown (single streaming connection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub start: i64,
    pub end: i64,
    pub pos: i64,
}

impl Segment {
    pub fn new(start: i64, end: i64, pos: i64) -> Segment {
        Segment { start, end, pos }
    }

    /// Bytes left, or -1 when the end is unknown.
    pub fn remaining(&self) -> i64 {
        if self.end < 0 {
            -1
        } else {
            (self.end - self.pos).max(0)
        }
    }

    pub fn done(&self) -> bool {
        self.end >= 0 && self.pos >= self.end
    }
}

fn default_category() -> String {
    "General".into()
}

fn default_size() -> i64 {
    -1
}

/// Seconds since the epoch, as a float like Python's `time.time()`.
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Download {
    pub id: i64,
    pub url: String,
    pub filename: String,
    pub save_dir: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub queue: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub referer: String,
    #[serde(default)]
    pub parent_page: String,
    #[serde(default = "default_size")]
    pub size: i64,
    #[serde(default)]
    pub downloaded: i64,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub resumable: Option<bool>,
    /// 0 = use the default from Options -> Connection.
    #[serde(default)]
    pub connections: i64,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default = "now")]
    pub date_added: f64,
    #[serde(default)]
    pub last_try: f64,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// Per-file limiter from the progress dialog, 0 = off.
    #[serde(default)]
    pub speed_limit_kbps: i64,
    #[serde(default)]
    pub queue_order: i64,
    /// Extra request headers (cookies, user agent… from the browser).
    #[serde(default)]
    pub headers: IndexMap<String, String>,
    /// Set for YouTube downloads: chosen variant and per-stream state. Kept as JSON; see `media`.
    #[serde(default)]
    pub media: Option<Map<String, Value>>,
    /// Optional checksum the user entered, as `"sha256:<lower-case hex>"`; `""` = no check.
    #[serde(default)]
    pub checksum: String,
    /// `""` (not checked) | verified | mismatch | skipped | computed
    #[serde(default)]
    pub checksum_result: String,
    /// The hex read from the file at the last check.
    #[serde(default)]
    pub checksum_actual: String,
    #[serde(default)]
    pub checksum_checked: f64,
    #[serde(default)]
    pub checksum_seconds: f64,
    /// The automatic download-again after a mismatch happens once.
    #[serde(default)]
    pub checksum_retried: bool,
    /// "Check when finished": checked even when Options turn verification off.
    #[serde(default)]
    pub checksum_force: bool,
    /// Fields written by another LID version; kept and written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Download {
    /// A new record with the defaults of 1.x.
    pub fn new(id: i64, url: &str, filename: &str, save_dir: &str) -> Download {
        Download {
            id,
            url: url.into(),
            filename: filename.into(),
            save_dir: save_dir.into(),
            category: default_category(),
            queue: String::new(),
            description: String::new(),
            referer: String::new(),
            parent_page: String::new(),
            size: -1,
            downloaded: 0,
            status: Status::Queued,
            resumable: None,
            connections: 0,
            segments: Vec::new(),
            date_added: now(),
            last_try: 0.0,
            error: String::new(),
            username: String::new(),
            password: String::new(),
            speed_limit_kbps: 0,
            queue_order: 0,
            headers: IndexMap::new(),
            media: None,
            checksum: String::new(),
            checksum_result: String::new(),
            checksum_actual: String::new(),
            checksum_checked: 0.0,
            checksum_seconds: 0.0,
            checksum_retried: false,
            checksum_force: false,
            extra: Map::new(),
        }
    }

    /// `save_dir` joined with `filename`.
    pub fn path(&self) -> String {
        crate::paths::join(&self.save_dir, &self.filename)
    }

    /// 0..1; a finished download of unknown size counts as 1.
    pub fn progress(&self) -> f64 {
        if self.size > 0 {
            (self.downloaded as f64 / self.size as f64).min(1.0)
        } else if self.status == Status::Complete {
            1.0
        } else {
            0.0
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("a Download always serialises")
    }

    pub fn from_json(v: Value) -> serde_json::Result<Download> {
        serde_json::from_value(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_rules() {
        assert_eq!(Segment::new(0, -1, 50).remaining(), -1);
        assert_eq!(Segment::new(0, 100, 40).remaining(), 60);
        assert_eq!(Segment::new(0, 100, 140).remaining(), 0);
        assert!(Segment::new(0, 100, 100).done());
        assert!(!Segment::new(0, -1, 100).done());
    }

    #[test]
    fn status_text_and_active() {
        assert_eq!(
            serde_json::to_value(Status::Downloading).unwrap(),
            "downloading"
        );
        let active: Vec<_> = [
            Status::Queued,
            Status::Connecting,
            Status::Downloading,
            Status::Paused,
            Status::Assembling,
            Status::Verifying,
            Status::Complete,
            Status::Error,
        ]
        .into_iter()
        .filter(|s| s.is_active())
        .collect();
        assert_eq!(
            active,
            [Status::Connecting, Status::Downloading, Status::Assembling]
        );
    }

    #[test]
    fn minimal_record_takes_defaults_and_keeps_unknown_fields() {
        let d = Download::from_json(serde_json::json!({
            "id": 3, "url": "u", "filename": "f", "save_dir": "/s", "from_the_future": [1, 2]
        }))
        .unwrap();
        assert_eq!(d.status, Status::Queued);
        assert_eq!(d.size, -1);
        assert_eq!(d.category, "General");
        assert_eq!(d.resumable, None);
        assert_eq!(d.path(), "/s/f");
        assert_eq!(d.to_json()["from_the_future"], serde_json::json!([1, 2]));
        assert!(Download::from_json(serde_json::json!({"id": 1, "url": "u"})).is_err());
    }

    /// Times are floats with full precision (Python writes the shortest exact form); they must survive a
    /// save and load bit for bit.
    #[test]
    fn float_times_round_trip_exactly() {
        for t in [
            1793950472.7764683,
            1793950473.1092653,
            0.1 + 0.2,
            1e-7,
            123456789.98765433,
        ] {
            let mut d = Download::new(1, "u", "f", "/s");
            d.date_added = t;
            let text = d.to_json().to_string();
            let back: Download = serde_json::from_str(&text).unwrap();
            assert_eq!(back.date_added.to_bits(), t.to_bits(), "{t} -> {text}");
        }
    }

    #[test]
    fn progress() {
        let mut d = Download::new(1, "u", "f", "/s");
        assert_eq!(d.progress(), 0.0);
        d.status = Status::Complete;
        assert_eq!(d.progress(), 1.0);
        d.size = 200;
        d.downloaded = 50;
        assert_eq!(d.progress(), 0.25);
    }
}
