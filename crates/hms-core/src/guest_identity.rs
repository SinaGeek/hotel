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
use getrandom::getrandom;
use std::sync::Mutex;

/// Guards the process-global `GUEST_FIELD_ENCRYPTION_KEY` env var so tests
/// that swap it cannot race with concurrent encryption/decryption.
static KEY_ENV_MUTEX: Mutex<()> = Mutex::new(());

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

fn env_key_name() -> &'static str {
    "GUEST_FIELD_ENCRYPTION_KEY"
}

/// Read and validate the guest field encryption key from the environment.
pub fn guest_field_encryption_key() -> Result<Vec<u8>, String> {
    let _guard = KEY_ENV_MUTEX.lock().unwrap();
    let value = std::env::var(env_key_name()).map_err(|_| {
        format!(
            "{} is not set; set it to a base64-encoded 32-byte key",
            env_key_name()
        )
    })?;
    let bytes = STANDARD
        .decode(value.trim())
        .map_err(|e| format!("{} is not valid base64: {}", env_key_name(), e))?;
    if bytes.len() != KEY_LEN {
        return Err(format!("{} must decode to exactly {} bytes", env_key_name(), KEY_LEN));
    }
    Ok(bytes)
}

fn cipher() -> Result<XChaCha20Poly1305, String> {
    let key = guest_field_encryption_key()?;
    XChaCha20Poly1305::new_from_slice(&key).map_err(|e| e.to_string())
}

/// Encrypt one guest identity field.
pub fn encrypt_guest_field(plaintext: &str) -> Result<String, String> {
    let cipher = cipher()?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| format!("nonce generation failed: {e}"))?;
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("guest field encryption failed: {e}"))?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(&out))
}

/// Decrypt one guest identity field.
pub fn decrypt_guest_field(ciphertext: &str) -> Result<String, String> {
    let decoded = STANDARD.decode(ciphertext).map_err(|e| format!("invalid ciphertext encoding: {e}"))?;
    if decoded.len() < NONCE_LEN {
        return Err("ciphertext too short".to_string());
    }
    let (nonce_bytes, ct) = decoded.split_at(NONCE_LEN);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = cipher()?;
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("guest field decryption failed: {e}"))?;
    String::from_utf8(plaintext).map_err(|e| format!("decrypted field is not valid UTF-8: {e}"))
}

/// Encrypt an optional guest identity field.
pub fn encrypt_option_guest_field(plaintext: Option<&str>) -> Result<Option<String>, String> {
    plaintext.map(|s| encrypt_guest_field(s)).transpose()
}

/// Decrypt an optional guest identity field.
pub fn decrypt_option_guest_field(ciphertext: Option<&str>) -> Result<Option<String>, String> {
    ciphertext.map(|s| decrypt_guest_field(s)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_env_key(value: String) {
        let _guard = KEY_ENV_MUTEX.lock().unwrap();
        std::env::set_var(env_key_name(), value);
    }

    fn set_key() {
        set_env_key(STANDARD.encode(&[0x42u8; KEY_LEN]));
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        set_key();
        let plaintext = "DE-12345678";
        let ciphertext = encrypt_guest_field(plaintext).unwrap();
        assert_ne!(ciphertext, plaintext);
        assert_eq!(decrypt_guest_field(&ciphertext).unwrap(), plaintext);
    }

    #[test]
    fn empty_field_round_trip() {
        set_key();
        let ciphertext = encrypt_guest_field("").unwrap();
        assert_eq!(decrypt_guest_field(&ciphertext).unwrap(), "");
    }

    #[test]
    fn optional_fields_round_trip() {
        set_key();
        assert!(encrypt_option_guest_field(None).unwrap().is_none());
        let ct = encrypt_option_guest_field(Some("phone")).unwrap().unwrap();
        assert_eq!(decrypt_option_guest_field(Some(&ct)).unwrap().unwrap(), "phone");
        assert!(decrypt_option_guest_field(None).unwrap().is_none());
    }

    #[test]
    fn wrong_key_fails() {
        set_key();
        let ct = encrypt_guest_field("secret").unwrap();
        set_env_key(STANDARD.encode(&[0x43u8; KEY_LEN]));
        assert!(decrypt_guest_field(&ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        set_key();
        let ct = encrypt_guest_field("secret").unwrap();
        let mut ct_bytes = ct.into_bytes();
        if let Some(b) = ct_bytes.get_mut(30) {
            *b ^= 0xff;
        }
        let tampered = STANDARD.encode(&ct_bytes);
        assert!(decrypt_guest_field(&tampered).is_err());
    }

    #[test]
    fn short_key_is_rejected() {
        set_env_key(STANDARD.encode(&[0u8; 33]));
        assert!(guest_field_encryption_key().is_err());
    }
}
