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
        Self {
            max_requests,
            window_ms,
            buckets: HashMap::new(),
            // safety valve so attackers cannot grow the map without bound
            max_keys: 100_000,
        }
    }

    /// Returns `true` when the request is allowed, `false` when rate-limited.
    /// Each call advances the per-key window when it has expired.
    pub fn check(&mut self, key: &str) -> bool {
        let now = Instant::now();

        // Evict everything if the map grew too large (crude but bounded).
        if self.buckets.len() >= self.max_keys {
            let expired = |b: &Bucket| {
                let elapsed = now.duration_since(b.window_start).as_millis() as u64;
                elapsed < self.window_ms
            };
            self.buckets.retain(|_, b| expired(b));
            if self.buckets.len() >= self.max_keys {
                self.buckets.clear();
            }
        }

        let bucket = self
            .buckets
            .entry(key.to_string())
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
}
