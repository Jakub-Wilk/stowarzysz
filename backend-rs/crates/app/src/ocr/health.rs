//! How each OCR candidate has been doing, in this process's memory: a circuit breaker plus
//! moving averages for the logs. No active probing: every call costs money, so the only probe is
//! one real request let through after a cooldown.
//!
//! - *Closed*: tried in turn. Three retryable failures in a row open it.
//! - *Open*: skipped until its cooldown ends (15 s, doubling per failed probe up to 10 min,
//!   ±20% jitter so processes don't probe in lockstep).
//! - *Half-open*: the first request after the cooldown goes through; success closes it, failure
//!   opens it again for longer.

use std::time::{Duration, Instant};

pub const FAILURES_TO_OPEN: u32 = 3;
pub const FIRST_COOLDOWN: Duration = Duration::from_secs(15);
pub const MAX_COOLDOWN: Duration = Duration::from_mins(10);
/// Weight of the newest sample in the moving averages.
const SMOOTHING: f64 = 0.2;

#[derive(Debug, Clone)]
pub struct Health {
    failures: u32,
    open_until: Option<Instant>,
    cooldown: Duration,
    probing: bool,
    /// Moving average of successes (1) and failures (0).
    pub success_rate: f64,
    /// Moving average of successful calls' latency.
    pub latency: Duration,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            failures: 0,
            open_until: None,
            cooldown: FIRST_COOLDOWN,
            probing: false,
            success_rate: 1.0,
            latency: Duration::ZERO,
        }
    }
}

/// `duration` scaled by a factor in 0.8..1.2 taken from `seed`.
fn jittered(duration: Duration, seed: u64) -> Duration {
    #[allow(clippy::cast_precision_loss)] // a jitter factor
    let factor = 0.8 + (seed % 401) as f64 / 1000.0;
    duration.mul_f64(factor)
}

impl Health {
    /// Whether a request may go to this candidate now; a half-open breaker admits one probe.
    pub fn admit(&mut self, now: Instant) -> bool {
        match self.open_until {
            None => true,
            Some(until) if now < until => false,
            Some(_) if self.probing => false,
            Some(_) => {
                self.probing = true;
                true
            }
        }
    }

    /// When this candidate can next be tried (now if it is closed).
    pub fn available_at(&self, now: Instant) -> Instant {
        match self.open_until {
            Some(until) if until > now => until,
            _ => now,
        }
    }

    pub fn is_open(&self, now: Instant) -> bool {
        self.open_until
            .is_some_and(|until| now < until || self.probing)
    }

    pub fn succeeded(&mut self, latency: Duration) {
        self.failures = 0;
        self.open_until = None;
        self.cooldown = FIRST_COOLDOWN;
        self.probing = false;
        self.success_rate = self.success_rate.mul_add(1.0 - SMOOTHING, SMOOTHING);
        self.latency = if self.latency.is_zero() {
            latency
        } else {
            self.latency
                .mul_f64(1.0 - SMOOTHING)
                .saturating_add(latency.mul_f64(SMOOTHING))
        };
    }

    /// A retryable failure (overloaded, timed out, unreachable).
    pub fn failed(&mut self, now: Instant, seed: u64) {
        self.failures += 1;
        self.success_rate *= 1.0 - SMOOTHING;
        if self.probing {
            self.probing = false;
            self.cooldown = (self.cooldown * 2).min(MAX_COOLDOWN);
            self.open_until = Some(now + jittered(self.cooldown, seed));
        } else if self.failures >= FAILURES_TO_OPEN && self.open_until.is_none() {
            self.open_until = Some(now + jittered(self.cooldown, seed));
        }
    }

    /// The candidate cannot work as configured (bad key, unknown model): rest it the longest.
    pub fn misconfigured(&mut self, now: Instant) {
        self.probing = false;
        self.failures += 1;
        self.cooldown = MAX_COOLDOWN;
        self.open_until = Some(now + MAX_COOLDOWN);
    }

    /// A request that never got an answer (cancelled by the caller or a faster hedge): no
    /// verdict, but a probe slot it held is released.
    pub fn abandoned(&mut self) {
        self.probing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_failures_open_the_breaker_and_a_probe_closes_it() {
        let start = Instant::now();
        let mut health = Health::default();
        for _ in 0..2 {
            health.failed(start, 200);
            assert!(health.admit(start), "still closed");
        }
        health.failed(start, 200);
        assert!(!health.admit(start) && health.is_open(start));
        let later = start + FIRST_COOLDOWN; // jitter factor 1.0 for seed 200
        assert!(health.admit(later), "half-open lets one probe through");
        assert!(!health.admit(later), "but only one");
        health.succeeded(Duration::from_millis(800));
        assert!(health.admit(later) && !health.is_open(later));
        assert_eq!(health.latency, Duration::from_millis(800));
    }

    #[test]
    fn a_failed_probe_doubles_the_cooldown_up_to_the_cap() {
        let mut now = Instant::now();
        let mut health = Health::default();
        for _ in 0..FAILURES_TO_OPEN {
            health.failed(now, 200);
        }
        let mut expected = FIRST_COOLDOWN;
        for _ in 0..8 {
            now = health.available_at(now);
            assert!(health.admit(now));
            health.failed(now, 200);
            expected = (expected * 2).min(MAX_COOLDOWN);
            assert_eq!(health.available_at(now) - now, expected);
        }
        assert_eq!(expected, MAX_COOLDOWN);
    }

    #[test]
    fn misconfiguration_rests_the_longest_and_jitter_stays_within_a_fifth() {
        let now = Instant::now();
        let mut health = Health::default();
        health.misconfigured(now);
        assert_eq!(health.available_at(now) - now, MAX_COOLDOWN);
        for seed in [0, 1, 400, 401, 12_345, u64::MAX] {
            let d = jittered(Duration::from_secs(100), seed);
            assert!(d >= Duration::from_secs(80) && d <= Duration::from_secs(120));
        }
    }
}
