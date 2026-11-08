//! Guest identity field encryption at rest.
//!
//! The guest model already stores sensitive fields as `*_encrypted` columns.
//! This module encrypts/decrypts them before they reach SQLite. The encryption
//! key is read from `GUEST_FIELD_ENCRYPTION_KEY` (base64-encoded 32 bytes).
//! Without that variable the guest write path returns an error rather than
//! storing plaintext identity data.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
#[cfg(test)]
use std::sync::Mutex;

/// Serializes tests that swap the process-global key. Production code never
/// mutates the environment, so the reader does not take this lock (that would
/// deadlock callers that already hold it while re-encrypting).
#[cfg(test)]
pub(crate) static KEY_ENV_MUTEX: Mutex<()> = Mutex::new(());

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

fn env_key_name() -> &'static str {
    "GUEST_FIELD_ENCRYPTION_KEY"
}

/// Decode a base64 32-byte key. Public so operator tooling (key rotation)
/// can validate keys without touching process-global environment state.
pub fn decode_key(value: &str) -> Result<Vec<u8>, String> {
    let bytes = STANDARD
        .decode(value.trim())
        .map_err(|e| format!("invalid base64 key: {e}"))?;
    if bytes.len() != KEY_LEN {
        return Err(format!("key must decode to exactly {} bytes", KEY_LEN));
    }
    Ok(bytes)
}

/// Read and validate the guest field encryption key from the environment.
pub fn guest_field_encryption_key() -> Result<Vec<u8>, String> {
    let value = std::env::var(env_key_name()).map_err(|_| {
        format!(
            "{} is not set; set it to a base64-encoded 32-byte key",
            env_key_name()
        )
    })?;
    decode_key(&value).map_err(|e| format!("{}: {}", env_key_name(), e))
}

/// Encrypt one guest identity field with an explicit key.
pub fn encrypt_guest_field_with_key(plaintext: &str, key: &[u8]) -> Result<String, String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| format!("nonce generation failed: {e}"))?;
    let nonce = XNonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("guest field encryption failed: {e}"))?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(&out))
}

/// Decrypt one guest identity field with an explicit key.
pub fn decrypt_guest_field_with_key(ciphertext: &str, key: &[u8]) -> Result<String, String> {
    let decoded = STANDARD
        .decode(ciphertext)
        .map_err(|e| format!("invalid ciphertext encoding: {e}"))?;
    if decoded.len() < NONCE_LEN {
        return Err("ciphertext too short".to_string());
    }
    let (nonce_bytes, ct) = decoded.split_at(NONCE_LEN);
    let nonce_bytes: [u8; NONCE_LEN] = nonce_bytes
        .try_into()
        .map_err(|_| "invalid nonce length".to_string())?;
    let nonce = XNonce::from(nonce_bytes);
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let plaintext = cipher
        .decrypt(&nonce, ct)
        .map_err(|e| format!("guest field decryption failed: {e}"))?;
    String::from_utf8(plaintext).map_err(|e| format!("decrypted field is not valid UTF-8: {e}"))
}

/// Encrypt one guest identity field with the configured key.
pub fn encrypt_guest_field(plaintext: &str) -> Result<String, String> {
    let key = guest_field_encryption_key()?;
    encrypt_guest_field_with_key(plaintext, &key)
}

/// Decrypt one guest identity field with the configured key.
pub fn decrypt_guest_field(ciphertext: &str) -> Result<String, String> {
    let key = guest_field_encryption_key()?;
    decrypt_guest_field_with_key(ciphertext, &key)
}

/// Encrypt an optional guest identity field.
pub fn encrypt_option_guest_field(plaintext: Option<&str>) -> Result<Option<String>, String> {
    plaintext.map(|s| encrypt_guest_field(s)).transpose()
}

/// Decrypt an optional guest identity field.
pub fn decrypt_option_guest_field(ciphertext: Option<&str>) -> Result<Option<String>, String> {
    ciphertext.map(|s| decrypt_guest_field(s)).transpose()
}

/// Encrypt an optional guest identity field with an explicit key.
pub fn encrypt_option_guest_field_with_key(
    plaintext: Option<&str>,
    key: &[u8],
) -> Result<Option<String>, String> {
    plaintext
        .map(|s| encrypt_guest_field_with_key(s, key))
        .transpose()
}

/// Decrypt an optional guest identity field with an explicit key.
pub fn decrypt_option_guest_field_with_key(
    ciphertext: Option<&str>,
    key: &[u8],
) -> Result<Option<String>, String> {
    ciphertext
        .map(|s| decrypt_guest_field_with_key(s, key))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; KEY_LEN] = [0x42u8; KEY_LEN];

    /// One env-based test holds the mutex for its whole body; everything else
    /// uses the explicit-key API so tests stay independent and parallel-safe.
    fn with_env_key<R>(value: &str, f: impl FnOnce() -> R) -> R {
        let _guard = KEY_ENV_MUTEX.lock().unwrap();
        std::env::set_var(env_key_name(), value);
        let result = f();
        std::env::remove_var(env_key_name());
        result
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let plaintext = "DE-12345678";
        let ciphertext = encrypt_guest_field_with_key(plaintext, &KEY).unwrap();
        assert_ne!(ciphertext, plaintext);
        assert_eq!(
            decrypt_guest_field_with_key(&ciphertext, &KEY).unwrap(),
            plaintext
        );
    }

    #[test]
    fn empty_field_round_trip() {
        let ciphertext = encrypt_guest_field_with_key("", &KEY).unwrap();
        assert_eq!(decrypt_guest_field_with_key(&ciphertext, &KEY).unwrap(), "");
    }

    #[test]
    fn optional_fields_round_trip() {
        assert!(encrypt_option_guest_field_with_key(None, &KEY)
            .unwrap()
            .is_none());
        let ct = encrypt_option_guest_field_with_key(Some("phone"), &KEY)
            .unwrap()
            .unwrap();
        assert_eq!(
            decrypt_option_guest_field_with_key(Some(&ct), &KEY)
                .unwrap()
                .unwrap(),
            "phone"
        );
        assert!(decrypt_option_guest_field_with_key(None, &KEY)
            .unwrap()
            .is_none());
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt_guest_field_with_key("secret", &KEY).unwrap();
        let other = [0x43u8; KEY_LEN];
        assert!(decrypt_guest_field_with_key(&ct, &other).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let ct = encrypt_guest_field_with_key("secret", &KEY).unwrap();
        let mut ct_bytes = ct.into_bytes();
        if let Some(b) = ct_bytes.get_mut(30) {
            *b ^= 0xff;
        }
        let tampered = STANDARD.encode(&ct_bytes);
        assert!(decrypt_guest_field_with_key(&tampered, &KEY).is_err());
    }

    #[test]
    fn env_key_is_used_by_the_default_api() {
        with_env_key(&STANDARD.encode(KEY), || {
            let ct = encrypt_guest_field("via-env").unwrap();
            assert_eq!(decrypt_guest_field(&ct).unwrap(), "via-env");
        });
    }

    #[test]
    fn missing_env_key_is_an_error() {
        let _guard = KEY_ENV_MUTEX.lock().unwrap();
        std::env::remove_var(env_key_name());
        assert!(encrypt_guest_field("x").is_err());
    }

    #[test]
    fn short_key_is_rejected() {
        assert!(decode_key(&STANDARD.encode([0u8; 33])).is_err());
        assert!(decode_key("not base64 !").is_err());
    }
}
