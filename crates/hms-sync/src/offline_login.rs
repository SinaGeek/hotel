//! Offline login bookkeeping (README §11.7).
//!
//! After a successful ONLINE login the desktop stores an
//! [`hms_core::sync::OfflineLoginCache`]: the argon2 password hash (the
//! verifier), role and permissions. Logins are then verifiable offline until
//! the cache expires (OFFLINE_LOGIN_DAYS, default 14).
//!
//! Lockout semantics: a user deactivated on the server is locked out at the
//! NEXT SYNC, not while offline — the engine surfaces that as
//! [`crate::engine::SyncFailure::AccountLocked`], and the caller discards the
//! cached credentials here.

use chrono::{DateTime, Utc};
use hms_core::roles::Role;
use hms_core::sync::{DEFAULT_OFFLINE_LOGIN_DAYS, OfflineLoginCache};
use uuid::Uuid;

/// Cache credentials after a successful online login. `password_hash` is the
/// argon2 PHC string the SERVER returned/stores — offline verification
/// re-hashes the typed password against it on the desktop side.
pub fn cache_after_online_login(
    user_id: Uuid,
    hotel_id: i64,
    username: impl Into<String>,
    role: Role,
    password_hash: impl Into<String>,
    now: DateTime<Utc>,
    offline_login_days: Option<i64>,
) -> OfflineLoginCache {
    OfflineLoginCache::create(
        user_id,
        hotel_id,
        username.into(),
        role,
        password_hash.into(),
        now,
        offline_login_days.unwrap_or(DEFAULT_OFFLINE_LOGIN_DAYS),
    )
}

/// Whether the cached credentials may still verify a login right now.
pub fn is_usable(cache: &OfflineLoginCache, now: DateTime<Utc>) -> bool {
    cache.is_valid(now)
}

/// Called when a sync reports the account as inactive/locked: expire the
/// cache immediately so the next login must go through the server.
pub fn lock_offline_login(cache: &mut OfflineLoginCache) {
    cache.invalidate();
}

/// Offline-login model: status returned by the desktop login flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineLoginResult {
    /// Verified against the cache (no network needed).
    Verified,
    /// Cache expired — online login required.
    Expired,
    /// Cache missing — online login required (first run).
    NoCache,
}
