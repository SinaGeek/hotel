//! Guest field encryption key rotation helpers.
//!
//! Rotating `GUEST_FIELD_ENCRYPTION_KEY` requires re-encrypting every stored
//! ciphertext with the new key. These helpers are pure: both keys are passed
//! explicitly, so operator tooling never mutates process-global env state
//! (the server binary lives in `crates/hms-server/src/bin/rotate_guest_keys.rs`).

use crate::guest_identity::{
    decrypt_guest_field_with_key, encrypt_guest_field_with_key,
};

/// Re-encrypt one field from `old_key` to `new_key`.
pub fn reencrypt_field_with_keys(
    old_ciphertext: &str,
    old_key: &[u8],
    new_key: &[u8],
) -> Result<String, String> {
    let plaintext = decrypt_guest_field_with_key(old_ciphertext, old_key)?;
    encrypt_guest_field_with_key(&plaintext, new_key)
}

/// Deprecated compatibility wrapper: decrypts with the key currently in
/// `GUEST_FIELD_ENCRYPTION_KEY` and encrypts with `new_key_bytes`.
/// Prefer [`reencrypt_field_with_keys`].
pub fn reencrypt_field(old_ciphertext: &str, new_key_bytes: &[u8]) -> Result<String, String> {
    let old_key = crate::guest_identity::guest_field_encryption_key()?;
    reencrypt_field_with_keys(old_ciphertext, &old_key, new_key_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_identity::{
        decrypt_guest_field_with_key, encrypt_guest_field_with_key, KEY_ENV_MUTEX,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    #[test]
    fn reencrypt_preserves_plaintext() {
        let old_key = [0x42u8; 32];
        let new_key = [0x43u8; 32];
        let plaintext = "DE-12345678";
        let old_ct = encrypt_guest_field_with_key(plaintext, &old_key).unwrap();
        let new_ct = reencrypt_field_with_keys(&old_ct, &old_key, &new_key).unwrap();
        assert_ne!(old_ct, new_ct);
        assert_eq!(
            decrypt_guest_field_with_key(&new_ct, &new_key).unwrap(),
            plaintext
        );
        // the old key must no longer open the rotated ciphertext
        assert!(decrypt_guest_field_with_key(&new_ct, &old_key).is_err());
    }

    #[test]
    fn env_wrapper_still_works() {
        let _guard = KEY_ENV_MUTEX.lock().unwrap();
        let old_key = [0x52u8; 32];
        let new_key = [0x53u8; 32];
        std::env::set_var("GUEST_FIELD_ENCRYPTION_KEY", STANDARD.encode(old_key));
        let old_ct = encrypt_guest_field_with_key("AB-1", &old_key).unwrap();
        let new_ct = reencrypt_field(&old_ct, &new_key).unwrap();
        assert_eq!(
            decrypt_guest_field_with_key(&new_ct, &new_key).unwrap(),
            "AB-1"
        );
        std::env::remove_var("GUEST_FIELD_ENCRYPTION_KEY");
    }
}
