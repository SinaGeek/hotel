//! Offline login (README §11.7): verify a typed password against the cached
//! argon2 verifier when the server cannot be reached. A deactivated user is
//! locked out at the NEXT SYNC (engine), not while offline.

use crate::local_store::LocalStore;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use hms_core::sync::OfflineLoginCache;
use uuid::Uuid;

/// Result of an offline login attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineAuth {
    /// Credentials verified against the cache; role/permission set included.
    Verified { user_id: Uuid, role: Box<str> },
    /// There is no cache for this machine yet — online login required.
    NoCache,
    /// Cache present but past OFFLINE_LOGIN_DAYS.
    Expired,
    /// Wrong password given.
    BadPassword,
}

/// Attempt an offline login for (hotel, username, password).
pub fn offline_login(
    store: &mut LocalStore,
    hotel_id: i64,
    username: &str,
    password: &str,
) -> Result<OfflineAuth, String> {
    let Some(cache) = store.load_offline_cache().map_err(|e| e.to_string())? else {
        return Ok(OfflineAuth::NoCache);
    };
    if !cache.is_valid(chrono::Utc::now()) {
        store.drop_offline_cache().ok();
        return Ok(OfflineAuth::Expired);
    }
    if cache.hotel_id != hotel_id || cache.username != username {
        return Ok(OfflineAuth::NoCache);
    }
    let parsed = PasswordHash::new(&cache.password_hash).map_err(|e| e.to_string())?;
    if Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok() {
        Ok(OfflineAuth::Verified {
            user_id: cache.user_id,
            role: cache.role.as_str().into(),
        })
    } else {
        Ok(OfflineAuth::BadPassword)
    }
}

/// Persist credentials after a successful online login (the webview login
/// path calls this through `tauri::command` so the cache always mirrors the
/// server's last answer).
pub fn cache_from_online_login(
    store: &mut LocalStore,
    user_id: Uuid,
    hotel_id: i64,
    username: &str,
    password_hash: &str,
    role: hms_core::roles::Role,
) -> Result<(), String> {
    let days: i64 = std::env::var("OFFLINE_LOGIN_DAYS")
        .ok()
        .and_then(|d| d.trim().parse().ok())
        .unwrap_or(hms_core::sync::DEFAULT_OFFLINE_LOGIN_DAYS);
    let cache = OfflineLoginCache::create(
        user_id,
        hotel_id,
        username.to_owned(),
        role,
        password_hash.to_owned(),
        chrono::Utc::now(),
        days,
    );
    store.save_offline_cache(&cache).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    use chrono::Duration;

    fn hash_of(pw: &[u8]) -> String {
        // fixed does-not-reuse salt is fine for a test-only verifier
        let salt = SaltString::encode_b64(b"0123456789abcdef").expect("salt");
        Argon2::default()
            .hash_password(pw, &salt)
            .expect("hash")
            .to_string()
    }

    #[test]
    fn verifies_password_and_matches_cache_identity() {
        let mut store = LocalStore::with_in_memory().expect("store");
        cache_from_online_login(
            &mut store,
            Uuid::now_v7(),
            4443,
            "hkdemo",
            &hash_of(b"S3cret!"),
            hms_core::roles::Role::Housekeeping,
        )
        .expect("cache saved");

        match offline_login(&mut store, 4443, "hkdemo", "S3cret!").expect("attempt") {
            OfflineAuth::Verified { role, .. } => assert_eq!(role.as_ref(), "housekeeping"),
            other => panic!("expected Verified, got {other:?}"),
        }
        assert_eq!(
            offline_login(&mut store, 4443, "hkdemo", "Wrong!").unwrap(),
            OfflineAuth::BadPassword
        );
        assert_eq!(
            offline_login(&mut store, 7, "hkdemo", "S3cret!").unwrap(),
            OfflineAuth::NoCache,
            "other hotel must not reuse this cache"
        );
    }

    #[test]
    fn expired_cache_is_decisive_and_wiped() {
        let mut store = LocalStore::with_in_memory().expect("store");
        let past = OfflineLoginCache::create(
            Uuid::now_v7(),
            4443,
            "hkdemo".into(),
            hms_core::roles::Role::Housekeeping,
            hash_of(b"S3cret!"),
            chrono::Utc::now() - Duration::days(15),
            hms_core::sync::DEFAULT_OFFLINE_LOGIN_DAYS,
        );
        store.save_offline_cache(&past).expect("save");

        assert_eq!(
            offline_login(&mut store, 4443, "hkdemo", "S3cret!").unwrap(),
            OfflineAuth::Expired
        );
        assert!(
            store.load_offline_cache().expect("load").is_none(),
            "expired cache must be wiped, not merely rejected"
        );
    }
}
