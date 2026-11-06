//! Text formats used in the download list and dialogs (`fmt.py` in 1.x).

use chrono::{Local, TimeZone};

use crate::models::{Download, Status};

/// `"512 Bytes"`, `"1.50 KB"` … up to TB; `""` for a negative (unknown) size.
pub fn size(n: i64) -> String {
    if n < 0 {
        return String::new();
    }
    if n < 1024 {
        return format!("{n} Bytes");
    }
    let mut v = n as f64;
    for unit in ["KB", "MB", "GB", "TB"] {
        v /= 1024.0;
        if v < 1024.0 || unit == "TB" {
            return format!("{v:.2} {unit}");
        }
    }
    String::new()
}

/// `"1.50 KB/sec"`; `""` when not moving.
pub fn rate(bytes_per_second: f64) -> String {
    if bytes_per_second > 0.0 {
        format!("{}/sec", size(bytes_per_second as i64))
    } else {
        String::new()
    }
}

/// `"1 hour(s) 2 min"`, `"2 min 5 sec"`, `"5 sec"`; `""` for a negative (unknown) time.
pub fn duration(seconds: i64) -> String {
    if seconds < 0 {
        return String::new();
    }
    let (h, rem) = (seconds / 3600, seconds % 3600);
    let (m, s) = (rem / 60, rem % 60);
    if h > 0 {
        format!("{h} hour(s) {m} min")
    } else if m > 0 {
        format!("{m} min {s} sec")
    } else {
        format!("{s} sec")
    }
}

/// `"Oct 25 09:41:30 2026"` in local time; `""` for 0 (never).
pub fn date(timestamp: f64) -> String {
    if timestamp == 0.0 {
        return String::new();
    }
    let secs = timestamp.trunc() as i64;
    match Local.timestamp_opt(secs, 0).single() {
        Some(t) => t.format("%b %d %H:%M:%S %Y").to_string(),
        None => String::new(),
    }
}

/// The Status column's text.
pub fn status(d: &Download) -> String {
    match d.status {
        Status::Complete => "Complete".into(),
        Status::Error => "Error".into(),
        Status::Connecting => "Connecting...".into(),
        Status::Assembling => "Assembling...".into(),
        Status::Verifying => "Verifying...".into(),
        Status::Queued if d.downloaded == 0 => String::new(),
        _ if d.size > 0 => format!("{:.2}%", 100.0 * d.downloaded as f64 / d.size as f64),
        _ => size(d.downloaded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of test_manager.py `test_formats`.
    #[test]
    fn formats() {
        assert_eq!(size(512), "512 Bytes");
        assert_eq!(size(1536), "1.50 KB");
        assert_eq!(size(5 * 1024_i64.pow(3)), "5.00 GB");
        assert_eq!(duration(125), "2 min 5 sec");
        assert_eq!(duration(3720), "1 hour(s) 2 min");
        let mut d = Download::new(1, "u", "f", "/tmp");
        d.size = 200;
        d.downloaded = 50;
        d.status = Status::Paused;
        assert_eq!(status(&d), "25.00%");
    }

    #[test]
    fn edges() {
        assert_eq!(size(-1), "");
        assert_eq!(size(0), "0 Bytes");
        assert_eq!(size(1023), "1023 Bytes");
        assert_eq!(size(1024), "1.00 KB");
        assert_eq!(size(3 * 1024_i64.pow(5)), "3072.00 TB");
        assert_eq!(rate(0.0), "");
        assert_eq!(rate(2048.7), "2.00 KB/sec");
        assert_eq!(duration(-1), "");
        assert_eq!(duration(0), "0 sec");
        assert_eq!(date(0.0), "");
        assert_eq!(date(1_793_000_000.0).len(), "Oct 25 09:41:30 2026".len());
        let mut d = Download::new(1, "u", "f", "/tmp");
        assert_eq!(status(&d), "");
        d.downloaded = 2048;
        assert_eq!(status(&d), "2.00 KB");
        for (st, text) in [
            (Status::Complete, "Complete"),
            (Status::Error, "Error"),
            (Status::Connecting, "Connecting..."),
            (Status::Assembling, "Assembling..."),
            (Status::Verifying, "Verifying..."),
        ] {
            d.status = st;
            assert_eq!(status(&d), text);
        }
    }
}
