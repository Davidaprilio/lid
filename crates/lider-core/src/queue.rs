//! The saved part of a queue: the Scheduler's "Schedule" and "Files in the queue" tabs
//! (`QueueSchedule` in `manager.py`). The running state of a queue lives in the manager.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueSchedule {
    pub max_parallel: i64,
    /// "Periodic synchronization" instead of "One-time downloading".
    pub periodic: bool,
    pub start_on_startup: bool,
    pub start_enabled: bool,
    pub start_time: String,
    pub daily: bool,
    /// YYYY-MM-DD for "Once at".
    pub once_date: String,
    /// Weekdays for "Daily", 0 = Monday.
    pub days: Vec<i64>,
    pub every_hours: i64,
    pub every_minutes: i64,
    pub stop_enabled: bool,
    pub stop_time: String,
    pub retries_enabled: bool,
    pub retries: i64,
    pub open_when_done: String,
    pub exit_when_done: bool,
    pub shutdown_when_done: bool,
    /// Fields written by another LID version; kept and written back unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for QueueSchedule {
    fn default() -> Self {
        QueueSchedule {
            max_parallel: 1,
            periodic: false,
            start_on_startup: false,
            start_enabled: false,
            start_time: "23:00".into(),
            daily: false,
            once_date: String::new(),
            days: (0..7).collect(),
            every_hours: 2,
            every_minutes: 0,
            stop_enabled: false,
            stop_time: "07:30".into(),
            retries_enabled: true,
            retries: 10,
            open_when_done: String::new(),
            exit_when_done: false,
            shutdown_when_done: false,
            extra: Map::new(),
        }
    }
}

/// `"HH:MM"` as a time; `None` when it is not one.
pub fn parse_hhmm(text: &str) -> Option<NaiveTime> {
    let (h, m) = text.split_once(':')?;
    NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
}

impl QueueSchedule {
    /// When "Start download at" fires next, to the minute, the way the scheduler checks it;
    /// `None` when it won't. `now` is local time.
    pub fn next_start(&self, now: NaiveDateTime) -> Option<NaiveDateTime> {
        if !self.start_enabled {
            return None;
        }
        let at = parse_hhmm(&self.start_time)?;
        let now = now.with_second(0)?.with_nanosecond(0)?;
        let today = now.date().and_time(at);
        if self.daily {
            return (0..8).map(|i| today + Duration::days(i)).find(|t| {
                *t >= now
                    && self
                        .days
                        .contains(&(t.weekday().num_days_from_monday() as i64))
            });
        }
        if !self.once_date.is_empty() {
            let t = NaiveDate::parse_from_str(&self.once_date, "%Y-%m-%d")
                .ok()?
                .and_time(at);
            return (t >= now).then_some(t);
        }
        Some(if today >= now {
            today
        } else {
            today + Duration::days(1)
        })
    }
}

/// A queue as stored in the `queues` table: `{"schedule": {...}}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QueueRecord {
    #[serde(default)]
    pub schedule: QueueSchedule,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    /// Port of test_manager.py `NextStartTests.test_once_daily_and_off`.
    #[test]
    fn once_daily_and_off() {
        let now = dt(2026, 10, 25, 9, 41, 30); // a Sunday
        let mut s = QueueSchedule {
            start_enabled: true,
            start_time: "01:00".into(),
            ..Default::default()
        };
        assert_eq!(s.next_start(now), Some(dt(2026, 10, 26, 1, 0, 0))); // no date: the next 01:00
        s.start_time = "09:41".into();
        assert_eq!(s.next_start(now), Some(dt(2026, 10, 25, 9, 41, 0))); // this minute still counts
        s.once_date = "2026-10-24".into();
        assert_eq!(s.next_start(now), None); // the day has passed
        let mut s = QueueSchedule {
            start_enabled: true,
            start_time: "08:00".into(),
            daily: true,
            days: vec![0, 2],
            ..Default::default()
        };
        assert_eq!(s.next_start(now), Some(dt(2026, 10, 26, 8, 0, 0))); // Monday
        s.days = vec![];
        assert_eq!(s.next_start(now), None);
        assert_eq!(QueueSchedule::default().next_start(now), None);
    }

    #[test]
    fn defaults_match_1x() {
        let s: QueueSchedule = serde_json::from_str("{}").unwrap();
        assert_eq!(s, QueueSchedule::default());
        assert_eq!(s.days, vec![0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(
            (s.start_time.as_str(), s.stop_time.as_str()),
            ("23:00", "07:30")
        );
        assert_eq!((s.max_parallel, s.every_hours, s.retries), (1, 2, 10));
        assert!(s.retries_enabled);
    }
}
