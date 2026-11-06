//! Speed limiting and measuring (`SpeedLimiter` and `SpeedMeter` in `engine.py`).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

/// Token bucket shared by every connection that should obey one limit. 0 = unlimited.
#[derive(Debug)]
pub struct SpeedLimiter {
    state: Mutex<Bucket>,
}

#[derive(Debug)]
struct Bucket {
    /// Bytes per second.
    rate: u64,
    tokens: f64,
    at: Instant,
}

impl SpeedLimiter {
    pub fn new(kbps: i64) -> SpeedLimiter {
        SpeedLimiter {
            state: Mutex::new(Bucket {
                rate: kbps.max(0) as u64 * 1024,
                tokens: 0.0,
                at: Instant::now(),
            }),
        }
    }

    fn bucket(&self) -> std::sync::MutexGuard<'_, Bucket> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Change the limit; the bucket starts empty again.
    pub fn set_kbps(&self, kbps: i64) {
        let mut b = self.bucket();
        b.rate = kbps.max(0) as u64 * 1024;
        b.tokens = 0.0;
        b.at = Instant::now();
    }

    /// Bytes per second, 0 when unlimited.
    pub fn rate(&self) -> u64 {
        self.bucket().rate
    }

    /// How long to wait after taking `n` bytes. The bucket holds at most 250 ms worth of bytes.
    pub fn charge(&self, n: usize) -> Duration {
        let mut b = self.bucket();
        if b.rate == 0 {
            return Duration::ZERO;
        }
        let rate = b.rate as f64;
        let now = Instant::now();
        b.tokens = (rate * 0.25).min(b.tokens + now.duration_since(b.at).as_secs_f64() * rate);
        b.at = now;
        b.tokens -= n as f64;
        let delay = -b.tokens / rate;
        if delay > 0.0 {
            Duration::from_secs_f64(delay)
        } else {
            Duration::ZERO
        }
    }

    /// Take `n` bytes and wait as long as the limit asks; a stop ends the wait early.
    pub async fn consume(&self, n: usize, stop: &CancellationToken) {
        let delay = self.charge(n);
        if !delay.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = stop.cancelled() => {}
            }
        }
    }
}

/// Transfer rate over a sliding window of 4 seconds.
#[derive(Debug)]
pub struct SpeedMeter {
    window: Duration,
    samples: Mutex<VecDeque<(Instant, u64)>>,
}

impl Default for SpeedMeter {
    fn default() -> Self {
        SpeedMeter::new(Duration::from_secs(4))
    }
}

impl SpeedMeter {
    pub fn new(window: Duration) -> SpeedMeter {
        SpeedMeter {
            window,
            samples: Mutex::new(VecDeque::new()),
        }
    }

    fn prune(&self, samples: &mut VecDeque<(Instant, u64)>, now: Instant) {
        while samples
            .front()
            .is_some_and(|(t, _)| now.duration_since(*t) > self.window)
        {
            samples.pop_front();
        }
    }

    pub fn add(&self, n: u64) {
        let now = Instant::now();
        let mut s = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        s.push_back((now, n));
        self.prune(&mut s, now);
    }

    /// Bytes per second: the bytes in the window over the time since the oldest sample (at least 1 s).
    pub fn rate(&self) -> f64 {
        let now = Instant::now();
        let mut s = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        self.prune(&mut s, now);
        let Some((oldest, _)) = s.front() else {
            return 0.0;
        };
        let span = now.duration_since(*oldest).as_secs_f64().max(1.0);
        s.iter().map(|(_, n)| *n as f64).sum::<f64>() / span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_never_waits() {
        let l = SpeedLimiter::new(0);
        assert_eq!(l.charge(10_000_000), Duration::ZERO);
        assert_eq!(SpeedLimiter::new(-5).rate(), 0);
    }

    #[test]
    fn limited_waits_for_the_bytes() {
        let l = SpeedLimiter::new(100); // 102400 B/s, bucket starts empty
        let d = l.charge(102_400);
        assert!(
            d > Duration::from_millis(900) && d <= Duration::from_secs(1),
            "{d:?}"
        );
        l.set_kbps(200);
        assert_eq!(l.rate(), 204_800);
    }

    #[test]
    fn meter_divides_by_at_least_a_second() {
        let m = SpeedMeter::default();
        assert_eq!(m.rate(), 0.0);
        m.add(1000);
        m.add(500);
        assert_eq!(m.rate(), 1500.0);
    }
}
