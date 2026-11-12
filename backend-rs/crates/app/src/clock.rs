//! The current time, injectable so deadlines and expiries are testable.

use std::fmt;
use std::sync::Mutex;

use chrono::{DateTime, TimeDelta, Utc};

/// Where "now" comes from. Use the clock for every business decision about time (deadlines,
/// expiry, reminders); the database's `now()` only for bookkeeping columns.
pub trait Clock: Send + Sync + fmt::Debug + 'static {
    fn now(&self) -> DateTime<Utc>;
}

/// The real time.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock that only moves when told to; for tests.
#[derive(Debug)]
pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(Mutex::new(now))
    }

    /// A fixed, arbitrary moment (rounded to microseconds, like Postgres stores it).
    pub fn at_default() -> Self {
        Self::new(DateTime::from_timestamp(1_798_000_000, 0).unwrap_or_default())
    }

    pub fn set(&self, now: DateTime<Utc>) {
        *self.lock() = now;
    }

    pub fn advance(&self, by: TimeDelta) {
        let mut now = self.lock();
        *now += by;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DateTime<Utc>> {
        // A poisoned clock only means another test thread panicked; the value is still valid.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_moves_only_when_told() {
        let clock = FixedClock::at_default();
        let start = clock.now();
        assert_eq!(clock.now(), start);
        clock.advance(TimeDelta::hours(72));
        assert_eq!(clock.now() - start, TimeDelta::hours(72));
    }
}
