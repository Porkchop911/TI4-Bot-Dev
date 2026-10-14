//! Time and nonces, injected.
//!
//! `std::time::Instant::now()` and `SystemTime::now()` panic on `wasm32-unknown-unknown`, so no
//! code in this crate reads them. The code that needs a time takes a [`Clock`]; the server
//! passes [`SystemClock`], the browser host passes one made from `Date.now()` and
//! `performance.now()`, a test passes [`ManualClock`].

use std::sync::atomic::{AtomicU64, Ordering};

/// A source of time.
pub trait Clock: Send + Sync {
    /// Milliseconds on a clock that never goes back, from an arbitrary start. For deadlines.
    fn monotonic_ms(&self) -> u64;
    /// Seconds since the Unix epoch (UTC). For the timestamps of events.
    fn unix_secs(&self) -> u64;

    /// `HH:MM:SS` (UTC), the timestamp that an event of the log carries.
    fn utc_time_string(&self) -> String {
        utc_time_string(self.unix_secs())
    }
}

/// `HH:MM:SS` of a Unix time.
#[must_use]
pub fn utc_time_string(unix_secs: u64) -> String {
    let hours = (unix_secs / 3600) % 24;
    let mins = (unix_secs / 60) % 60;
    let secs = unix_secs % 60;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

/// A deadline on a [`Clock`].
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    at_ms: u64,
}

impl Deadline {
    /// `after_ms` milliseconds from now.
    #[must_use]
    pub fn after(clock: &dyn Clock, after_ms: u64) -> Self {
        Self {
            at_ms: clock.monotonic_ms().saturating_add(after_ms),
        }
    }

    /// Whether the deadline has passed.
    #[must_use]
    pub fn passed(&self, clock: &dyn Clock) -> bool {
        clock.monotonic_ms() >= self.at_ms
    }
}

/// A source of opaque, hard to guess tokens (the nonce of a pending choice).
pub trait NonceSource: Send + Sync {
    /// The next nonce, as 16 lower case hex digits.
    fn next_nonce(&self) -> String;
}

/// The system clock. Not available on `wasm32`: the host there passes its own.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn monotonic_ms(&self) -> u64 {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        let start = START.get_or_init(Instant::now);
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn unix_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

/// A clock that moves only when told to. For tests and for replays that must not depend on time.
#[derive(Debug, Default)]
pub struct ManualClock {
    monotonic_ms: AtomicU64,
    unix_secs: AtomicU64,
}

impl ManualClock {
    #[must_use]
    pub const fn new(monotonic_ms: u64, unix_secs: u64) -> Self {
        Self {
            monotonic_ms: AtomicU64::new(monotonic_ms),
            unix_secs: AtomicU64::new(unix_secs),
        }
    }

    pub fn advance_ms(&self, ms: u64) {
        self.monotonic_ms.fetch_add(ms, Ordering::Relaxed);
        self.unix_secs.fetch_add(ms / 1000, Ordering::Relaxed);
    }
}

impl Clock for ManualClock {
    fn monotonic_ms(&self) -> u64 {
        self.monotonic_ms.load(Ordering::Relaxed)
    }

    fn unix_secs(&self) -> u64 {
        self.unix_secs.load(Ordering::Relaxed)
    }
}

/// Nonces from a counter run through a mixer: unique, not secret. Enough where no other player
/// can see the token (one human, offline) and for tests.
#[derive(Debug)]
pub struct CounterNonce {
    next: AtomicU64,
}

impl CounterNonce {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            next: AtomicU64::new(seed),
        }
    }
}

impl NonceSource for CounterNonce {
    fn next_nonce(&self) -> String {
        // splitmix64 finaliser over the counter.
        let mut z = self
            .next
            .fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed)
            .wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        format!("{z:016x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_follow_the_injected_clock() {
        let clock = ManualClock::new(10, 0);
        let deadline = Deadline::after(&clock, 100);
        assert!(!deadline.passed(&clock));
        clock.advance_ms(99);
        assert!(!deadline.passed(&clock));
        clock.advance_ms(1);
        assert!(deadline.passed(&clock));
    }

    #[test]
    fn the_time_string_is_the_utc_time_of_day() {
        assert_eq!(utc_time_string(3 * 3600 + 4 * 60 + 5), "03:04:05");
        assert_eq!(utc_time_string(86_400 + 59), "00:00:59");
    }

    #[test]
    fn counter_nonces_are_sixteen_hex_digits_and_distinct() {
        let source = CounterNonce::new(1);
        let (a, b) = (source.next_nonce(), source.next_nonce());
        assert_eq!(a.len(), 16);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
