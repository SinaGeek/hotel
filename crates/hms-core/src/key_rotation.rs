//! Guest field encryption key rotation utilities.
//!
//! When rotating the guest field encryption key, existing encrypted data must
//! be re-encrypted with the new key. This module provides helpers for that.

use crate::guest_identity::{decrypt_guest_field, encrypt_guest_field};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Re-encrypt a single field from the old key (already set in the environment)
/// to a new key. The caller must swap `GUEST_FIELD_ENCRYPTION_KEY` after all
/// fields are re-encrypted.
pub fn reencrypt_field(old_ciphertext: &str, new_key_bytes: &[u8]) -> Result<String, String> {
    let plaintext = decrypt_guest_field(old_ciphertext)?;
    // Temporarily swap the key env var to use the new key for encryption
    let old_key = std::env::var("GUEST_FIELD_ENCRYPTION_KEY").ok();
    std::env::set_var("GUEST_FIELD_ENCRYPTION_KEY", STANDARD.encode(new_key_bytes));
    let new_ciphertext = encrypt_guest_field(&plaintext);
    // Restore the old key
    if let Some(k) = old_key {
        std::env::set_var("GUEST_FIELD_ENCRYPTION_KEY", k);
    } else {
        std::env::remove_var("GUEST_FIELD_ENCRYPTION_KEY");
    }
    new_ciphertext
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use crate::guest_identity::KEY_ENV_MUTEX;

    #[test]
    fn reencrypt_preserves_plaintext() {
        let _guard = KEY_ENV_MUTEX.lock().unwrap();
        let old_key = [0x42u8; 32];
        let new_key = [0x43u8; 32];
        std::env::set_var("GUEST_FIELD_ENCRYPTION_KEY", STANDARD.encode(&old_key));
        let plaintext = "DE-12345678";
        let old_ct = encrypt_guest_field(plaintext).unwrap();
        let new_ct = reencrypt_field(&old_ct, &new_key).unwrap();
        std::env::set_var("GUEST_FIELD_ENCRYPTION_KEY", STANDARD.encode(&new_key));
        let decrypted = decrypt_guest_field(&new_ct).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
