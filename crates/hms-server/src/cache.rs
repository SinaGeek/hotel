//! G3: Read caching for high-volume read endpoints (`board`, `rooms`, `room-types`)
//! with tenant-scoped write invalidation.
//!
//! Cache entries are keyed by `(hotel_id, domain, query_key)` and stored in an
//! in-memory concurrent map with TTL and explicit eviction on write. Every
//! modifying mutation (reservation create/edit/cancel/reassign, room edit, price rule)
//! calls `invalidate_domain(hotel_id, domain)` ensuring reads are never stale.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheDomain {
    Board,
    Rooms,
    RoomTypes,
}

#[derive(Clone)]
struct CacheEntry {
    payload: Value,
    expires_at: Instant,
}

#[derive(Default)]
struct InnerStore {
    // (hotel_id, domain) -> map of query_key -> entry
    buckets: HashMap<(i64, CacheDomain), HashMap<String, CacheEntry>>,
}

#[derive(Clone, Default)]
pub struct CacheStore {
    inner: Arc<RwLock<InnerStore>>,
}

impl CacheStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieve a cached JSON response if present and not expired.
    pub fn get(&self, hotel_id: i64, domain: CacheDomain, key: &str) -> Option<Value> {
        let store = self.inner.read().unwrap_or_else(|p| p.into_inner());
        let bucket = store.buckets.get(&(hotel_id, domain))?;
        let entry = bucket.get(key)?;
        if Instant::now() < entry.expires_at {
            Some(entry.payload.clone())
        } else {
            None
        }
    }

    /// Cache a JSON response with a given TTL.
    pub fn set(
        &self,
        hotel_id: i64,
        domain: CacheDomain,
        key: &str,
        payload: Value,
        ttl: Duration,
    ) {
        let mut store = self.inner.write().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let bucket = store.buckets.entry((hotel_id, domain)).or_default();
        // Expired query windows should not accumulate forever (board keys vary
        // by date range). Keep the bucket bounded even for a long-running hotel.
        bucket.retain(|_, entry| entry.expires_at > now);
        if bucket.len() >= 256 && !bucket.contains_key(key) {
            if let Some(oldest_key) = bucket
                .iter()
                .min_by_key(|(_, entry)| entry.expires_at)
                .map(|(key, _)| key.clone())
            {
                bucket.remove(&oldest_key);
            }
        }
        bucket.insert(
            key.to_string(),
            CacheEntry {
                payload,
                expires_at: now + ttl,
            },
        );
    }

    /// Invalidate all cached queries for a given domain under a tenant.
    pub fn invalidate(&self, hotel_id: i64, domain: CacheDomain) {
        let mut store = self.inner.write().unwrap_or_else(|p| p.into_inner());
        store.buckets.remove(&(hotel_id, domain));
    }

    /// Invalidate all cached domains for a tenant.
    pub fn invalidate_hotel(&self, hotel_id: i64) {
        let mut store = self.inner.write().unwrap_or_else(|p| p.into_inner());
        store.buckets.retain(|&(h, _), _| h != hotel_id);
    }

    /// Count total cached entries across all tenants and domains.
    pub fn entry_count(&self) -> usize {
        let store = self.inner.read().unwrap_or_else(|p| p.into_inner());
        store.buckets.values().map(|b| b.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cache_get_set_and_expiry() {
        let cache = CacheStore::new();
        let val = json!({ "rooms": [1, 2, 3] });
        cache.set(
            4442,
            CacheDomain::Rooms,
            "all",
            val.clone(),
            Duration::from_millis(50),
        );
        assert_eq!(cache.get(4442, CacheDomain::Rooms, "all"), Some(val));

        // Different key is None
        assert_eq!(cache.get(4442, CacheDomain::Rooms, "other"), None);

        // Different domain is None
        assert_eq!(cache.get(4442, CacheDomain::Board, "all"), None);

        // Different tenant is None
        assert_eq!(cache.get(9999, CacheDomain::Rooms, "all"), None);

        // Expire
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(cache.get(4442, CacheDomain::Rooms, "all"), None);
    }

    #[test]
    fn query_window_bucket_is_bounded() {
        let cache = CacheStore::new();
        for i in 0..300 {
            cache.set(
                4442,
                CacheDomain::Board,
                &format!("window-{i}"),
                json!({ "window": i }),
                Duration::from_secs(60),
            );
        }
        assert_eq!(cache.entry_count(), 256);
    }

    #[test]
    fn write_invalidation_clears_domain_for_tenant_only() {
        let cache = CacheStore::new();
        let val1 = json!({ "occupancy": 0.85 });
        let val2 = json!({ "occupancy": 0.50 });

        // Populate two tenants
        cache.set(
            4442,
            CacheDomain::Board,
            "2026-11-01:2026-11-30",
            val1.clone(),
            Duration::from_secs(60),
        );
        cache.set(
            4443,
            CacheDomain::Board,
            "2026-11-01:2026-11-30",
            val2.clone(),
            Duration::from_secs(60),
        );

        assert_eq!(cache.entry_count(), 2);

        // Invalidate hotel 4442 board
        cache.invalidate(4442, CacheDomain::Board);

        // 4442 is gone, 4443 is untouched
        assert_eq!(
            cache.get(4442, CacheDomain::Board, "2026-11-01:2026-11-30"),
            None
        );
        assert_eq!(
            cache.get(4443, CacheDomain::Board, "2026-11-01:2026-11-30"),
            Some(val2)
        );
    }
}
