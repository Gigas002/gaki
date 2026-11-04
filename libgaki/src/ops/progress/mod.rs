//! Download/verify progress maths shared by every front end.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Percentage (one decimal) of `done` out of `total`; 0 when total is unknown.
pub fn percent_of(done: u64, total: u64) -> f64 {
    if total > 0 {
        ((done as f64 / total as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    }
}

/// Span the speed is averaged over.
const RATE_WINDOW: Duration = Duration::from_secs(5);

/// Tracks the transfer rate across successive progress samples, deriving a
/// lightly smoothed speed (bytes/sec) and an ETA.
#[derive(Debug, Default)]
pub struct RateTracker {
    /// `(time, bytes)` within the window.
    samples: VecDeque<(Instant, u64)>,
    speed_bps: f64,
}

impl RateTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every sample (e.g. when an operation is re-queued).
    pub fn reset(&mut self) {
        self.samples.clear();
        self.speed_bps = 0.0;
    }

    /// Feed the latest cumulative `bytes` (out of `total`); returns
    /// `(speed_bps, eta_seconds)`, averaged over the last few seconds.
    pub fn sample(&mut self, bytes: u64, total: u64) -> (f64, Option<u64>) {
        self.sample_at(Instant::now(), bytes, total)
    }

    /// [`Self::sample`] at an explicit time.
    pub fn sample_at(&mut self, now: Instant, bytes: u64, total: u64) -> (f64, Option<u64>) {
        // A counter reset starts over.
        if self.samples.back().is_some_and(|&(_, b)| bytes < b) {
            self.samples.clear();
        }
        self.samples.push_back((now, bytes));
        // Keep one point older than the window.
        while self.samples.len() > 2 && now.duration_since(self.samples[1].0) >= RATE_WINDOW {
            self.samples.pop_front();
        }
        let (t0, b0) = self.samples[0];
        let dt = now.duration_since(t0).as_secs_f64();
        if dt >= 0.10 {
            self.speed_bps = (bytes - b0) as f64 / dt;
        }
        let eta = if self.speed_bps > 1.0 && total > bytes {
            Some(((total - bytes) as f64 / self.speed_bps).round() as u64)
        } else {
            None
        };
        (self.speed_bps, eta)
    }
}

#[cfg(test)]
mod tests;
