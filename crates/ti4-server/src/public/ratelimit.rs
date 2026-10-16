//! Per-client-IP token buckets for the public playtest instance.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Which family of endpoints a request is counted against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitKind {
    GameCreate,
    LobbyJoin,
    WsConnect,
    Batch,
    Heavy,
    Feedback,
    Password,
    General,
}

/// Bucket size and refill speed.
#[derive(Debug, Clone, Copy)]
pub struct RateSpec {
    pub capacity: f64,
    pub per_second: f64,
}

impl RateSpec {
    #[must_use]
    pub const fn new(capacity: f64, per_second: f64) -> Self {
        Self {
            capacity,
            per_second,
        }
    }
}

/// One spec per [`LimitKind`].
#[derive(Debug, Clone)]
pub struct RateLimits {
    pub game_create: RateSpec,
    pub lobby_join: RateSpec,
    pub ws_connect: RateSpec,
    pub batch: RateSpec,
    pub heavy: RateSpec,
    pub feedback: RateSpec,
    pub password: RateSpec,
    pub general: RateSpec,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            // 6 games in a burst, then one per 10 minutes.
            game_create: RateSpec::new(6.0, 1.0 / 600.0),
            lobby_join: RateSpec::new(30.0, 0.2),
            ws_connect: RateSpec::new(40.0, 0.5),
            batch: RateSpec::new(60.0, 10.0),
            heavy: RateSpec::new(20.0, 0.3),
            // 10 reports an hour.
            feedback: RateSpec::new(10.0, 10.0 / 3600.0),
            // 8 attempts, then one per 30 seconds.
            password: RateSpec::new(8.0, 1.0 / 30.0),
            general: RateSpec::new(300.0, 30.0),
        }
    }
}

impl RateLimits {
    /// The limits from the environment: `TI4_RATE_LIMITS=off` lifts every per-IP limit except the
    /// password attempts (brute-force protection); anything else keeps the defaults.
    #[must_use]
    pub fn from_env() -> Self {
        let off = std::env::var("TI4_RATE_LIMITS").is_ok_and(|v| {
            matches!(
                v.trim().to_lowercase().as_str(),
                "off" | "0" | "false" | "none"
            )
        });
        let mut limits = Self::default();
        if off {
            let open = RateSpec::new(1e9, 1e9);
            limits.game_create = open;
            limits.lobby_join = open;
            limits.ws_connect = open;
            limits.batch = open;
            limits.heavy = open;
            limits.feedback = open;
            limits.general = open;
        }
        limits
    }

    fn spec(&self, kind: LimitKind) -> RateSpec {
        match kind {
            LimitKind::GameCreate => self.game_create,
            LimitKind::LobbyJoin => self.lobby_join,
            LimitKind::WsConnect => self.ws_connect,
            LimitKind::Batch => self.batch,
            LimitKind::Heavy => self.heavy,
            LimitKind::Feedback => self.feedback,
            LimitKind::Password => self.password,
            LimitKind::General => self.general,
        }
    }
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Token buckets keyed by (kind, client IP).
pub struct RateLimiter {
    limits: RateLimits,
    buckets: Mutex<HashMap<(LimitKind, IpAddr), Bucket>>,
}

impl RateLimiter {
    #[must_use]
    pub fn new(limits: RateLimits) -> Self {
        Self {
            limits,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token; `Err(retry_after)` when the bucket is empty.
    pub fn check(&self, kind: LimitKind, ip: IpAddr) -> Result<(), Duration> {
        let spec = self.limits.spec(kind);
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect("rate limiter lock");
        if buckets.len() > 20_000 {
            // Drop buckets that have been idle long enough to be full again.
            buckets.retain(|(k, _), b| {
                let s = self.limits.spec(*k);
                b.tokens + now.duration_since(b.last).as_secs_f64() * s.per_second < s.capacity
            });
        }
        let bucket = buckets.entry((kind, ip)).or_insert(Bucket {
            tokens: spec.capacity,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * spec.per_second).min(spec.capacity);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait = (1.0 - bucket.tokens) / spec.per_second.max(1e-9);
            Err(Duration::from_secs_f64(wait.min(86_400.0)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_runs_dry_then_refuses_per_ip() {
        let mut limits = RateLimits::default();
        limits.feedback = RateSpec::new(2.0, 0.001);
        let limiter = RateLimiter::new(limits);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        assert!(limiter.check(LimitKind::Feedback, a).is_ok());
        assert!(limiter.check(LimitKind::Feedback, a).is_ok());
        assert!(limiter.check(LimitKind::Feedback, a).is_err());
        assert!(limiter.check(LimitKind::Feedback, b).is_ok());
        assert!(limiter.check(LimitKind::GameCreate, a).is_ok());
    }
}
