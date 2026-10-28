//! Offline login cache semantics (README §11.7): 14-day default expiry,
//! refresh on re-login, and lockout at the next sync.

use chrono::{Duration, Utc};
use hms_core::roles::Role;
use hms_core::sync::DEFAULT_OFFLINE_LOGIN_DAYS;
use hms_sync::offline_login::*;

fn online_login_at(now: i64) -> hms_core::sync::OfflineLoginCache {
    cache_after_online_login(
        uuid::Uuid::now_v7(),
        4443,
        "hkdemo",
        Role::Housekeeping,
        "$argon2id$v=19$m=19456,t=2,p=1$tester$hash",
        Utc::now() + Duration::seconds(now),
        None, // OFFLINE_LOGIN_DAYS default
    )
}

#[test]
fn default_expiry_is_fourteen_days() {
    assert_eq!(DEFAULT_OFFLINE_LOGIN_DAYS, 14);
    let t = Utc::now();
    let cache = online_login_at(0);
    assert_eq!(cache.hotel_id, 4443);
    assert!(matches!(cache.role, Role::Housekeeping));
    // usable for 14 days minus a second
    assert!(is_usable(&cache, t + Duration::days(14) - Duration::seconds(1)));
    // expired one second past the window
    assert!(!is_usable(&cache, t + Duration::days(14) + Duration::seconds(1)));
}

#[test]
fn explicit_configured_window_overrides_the_default() {
    let t = Utc::now();
    let cache = cache_after_online_login(
        uuid::Uuid::now_v7(),
        4443,
        "loadmgr",
        Role::Manager,
        "$argon2id$test",
        t,
        Some(30),
    );
    assert!(is_usable(&cache, t + Duration::days(29)));
    assert!(!is_usable(&cache, t + Duration::days(31)));
}

#[test]
fn refresh_extends_the_latest_expiry() {
    let t = Utc::now();
    let cache = online_login_at(0).refresh(t + Duration::days(10), DEFAULT_OFFLINE_LOGIN_DAYS);
    assert!(is_usable(&cache, t + Duration::days(23)), "10 + 14 - 1");
    assert!(!is_usable(&cache, t + Duration::days(25)));
}

#[test]
fn lockout_at_next_sync_expires_the_cache_immediately() {
    let t = Utc::now();
    let mut cache = online_login_at(0);
    assert!(is_usable(&cache, t));
    // The engine reported AccountLocked (user deactivated on the server):
    // the desktop wipes validity so the next login must be online.
    lock_offline_login(&mut cache);
    assert!(!is_usable(&cache, t), "locked: no offline login anymore");
}

#[test]
fn cache_keeps_the_verifier_for_the_desktop_hasher() {
    let cache = online_login_at(0);
    // The desktop re-hashes the typed password against this PHC string:
    // it must never store the plaintext.
    assert!(cache.password_hash.starts_with("$argon2"));
    assert!(!cache.username.is_empty());
}
