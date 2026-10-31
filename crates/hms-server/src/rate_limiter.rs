//! Fixed-window rate limiter, keyed by client identity (e.g. IP or username + IP).
//! In-memory only: acceptable for a single-node deployment; swap for a shared
//! store if the server is ever scaled horizontally.

use std::collections::HashMap;
use std::time::Instant;

pub struct RateLimiter {
    max_requests: usize,
    window_ms: u64,
    /// key -> (window start, requests seen in this window)
    buckets: HashMap<String, Bucket>,
    max_keys: usize,
}

struct Bucket {
    window_start: Instant,
    count: usize,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_ms: u64) -> Self {
        Self::with_max_keys(max_requests, window_ms, 100_000)
    }

    fn with_max_keys(max_requests: usize, window_ms: u64, max_keys: usize) -> Self {
        Self {
            max_requests,
            window_ms,
            buckets: HashMap::new(),
            // safety valve so attackers cannot grow the map without bound
            max_keys: max_keys.max(1),
        }
    }

    /// Returns `true` when the request is allowed, `false` when rate-limited.
    /// Each call advances the per-key window when it has expired.
    pub fn check(&mut self, key: &str) -> bool {
        const OVERFLOW_KEY: &str = "\0overflow";
        let now = Instant::now();

        // Evict only expired buckets. Clearing all live buckets at capacity
        // would let an attacker reset other clients' limits with key spray.
        if self.buckets.len() >= self.max_keys && !self.buckets.contains_key(key) {
            self.buckets.retain(|_, b| {
                (now.duration_since(b.window_start).as_millis() as u64) < self.window_ms
            });
        }

        // Once capacity remains full, share one bounded fallback bucket among
        // new keys. This preserves the memory cap and does not reset live limits.
        let key_capacity = self.max_keys.saturating_sub(1);
        let effective_key = if !self.buckets.contains_key(key) && self.buckets.len() >= key_capacity {
            OVERFLOW_KEY
        } else {
            key
        };
        let bucket = self
            .buckets
            .entry(effective_key.to_string())
            .and_modify(|b| {
                if now.duration_since(b.window_start).as_millis() as u64 >= self.window_ms {
                    b.window_start = now;
                    b.count = 0;
                }
            })
            .or_insert_with(|| Bucket {
                window_start: now,
                count: 0,
            });

        bucket.count += 1;
        bucket.count <= self.max_requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_within_limit() {
        let mut rl = RateLimiter::new(3, 60_000);
        assert!(rl.check("ip1"));
        assert!(rl.check("ip1"));
        assert!(rl.check("ip1"));
    }

    #[test]
    fn blocks_over_limit() {
        let mut rl = RateLimiter::new(3, 60_000);
        assert!(rl.check("ip1"));
        assert!(rl.check("ip1"));
        assert!(rl.check("ip1"));
        assert!(!rl.check("ip1"));
    }

    #[test]
    fn keys_are_independent() {
        let mut rl = RateLimiter::new(1, 60_000);
        assert!(rl.check("ip1"));
        assert!(!rl.check("ip1"));
        assert!(rl.check("ip2"));
    }

    #[test]
    fn key_spray_does_not_reset_live_buckets_or_exceed_cap() {
        let mut rl = RateLimiter::with_max_keys(2, 60_000, 3);
        assert!(rl.check("trusted"));
        assert!(rl.check("spray-1"));
        // New identities share the overflow bucket at capacity.
        assert!(rl.check("spray-2"));
        assert!(rl.check("spray-3"));
        assert!(!rl.check("spray-4"));
        assert!(rl.buckets.len() <= 3);
        // Existing known identity retains its own count; key spray did not reset it.
        assert!(rl.check("trusted"));
        assert!(!rl.check("trusted"));
        assert!(rl.buckets.len() <= 3);
    }

    #[test]
    fn overflow_capacity_of_one_stays_bounded() {
        let mut rl = RateLimiter::with_max_keys(1, 60_000, 1);
        assert!(rl.check("first"));
        assert!(!rl.check("second"));
        assert!(!rl.check("third"));
        assert_eq!(rl.buckets.len(), 1);
    }
}
