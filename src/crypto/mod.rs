//! Cryptographic subsystem for dotling.
//!
//! Provides password-based encryption using battle-tested crates:
//! - **Argon2id** for key derivation (memory-hard, resists GPU attacks)
//! - **ChaCha20-Poly1305** AEAD for authenticated encryption
//! - **CSPRNG** via `rand` for salt/nonce generation

pub mod vault;

use base64::prelude::*;
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use rand::Rng;

use crate::error::{Error, Result};

// ── Constants ─────────────────────────────────────────────────────

const HEADER: &str = "DOTLING-ENC-V1";
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;

// ── High-level encrypt / decrypt API ──────────────────────────────

/// Encrypt data with a password.
///
/// Output format (text):
/// ```text
/// DOTLING-ENC-V1
/// <32-byte salt as hex>
/// <12-byte nonce as hex>
/// <base64-encoded ciphertext+tag>
/// ```
pub fn encrypt(data: &[u8], password: &str) -> Result<Vec<u8>> {
    let salt = random_bytes::<SALT_LEN>();
    let nonce_bytes = random_bytes::<NONCE_LEN>();

    let key = derive_key(password, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| Error::Crypto(format!("encryption failed: {e}")))?;

    let output = format!(
        "{HEADER}\n{}\n{}\n{}\n",
        hex_encode(&salt),
        hex_encode(&nonce_bytes),
        BASE64_STANDARD.encode(&ciphertext),
    );

    Ok(output.into_bytes())
}

/// Decrypt data with a password.
///
/// Expects the `DOTLING-ENC-V1` format produced by [`encrypt`].
pub fn decrypt(data: &[u8], password: &str) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(data)
        .map_err(|_| Error::Crypto("invalid UTF-8 in encrypted file".into()))?;

    let lines: Vec<&str> = text.lines().collect();

    if lines.len() < 4 || lines[0] != HEADER {
        return Err(Error::Crypto(
            "not a valid dotling encrypted file (expected DOTLING-ENC-V1 header)".into(),
        ));
    }

    let salt = hex_decode(lines[1]).map_err(|e| Error::Crypto(format!("invalid salt: {e}")))?;
    let nonce_bytes =
        hex_decode(lines[2]).map_err(|e| Error::Crypto(format!("invalid nonce: {e}")))?;
    let ciphertext = BASE64_STANDARD
        .decode(lines[3])
        .map_err(|e| Error::Crypto(format!("invalid payload: {e}")))?;

    if salt.len() != SALT_LEN {
        return Err(Error::Crypto(format!(
            "expected {SALT_LEN}-byte salt, got {}",
            salt.len()
        )));
    }
    if nonce_bytes.len() != NONCE_LEN {
        return Err(Error::Crypto(format!(
            "expected {NONCE_LEN}-byte nonce, got {}",
            nonce_bytes.len()
        )));
    }

    let key = derive_key(password, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| Error::Crypto("decryption failed — wrong password or corrupted data".into()))
}

// ── Internal helpers ──────────────────────────────────────────────

/// Derive a 32-byte key from a password + salt using Argon2id.
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    use argon2::Argon2;

    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Crypto(format!("key derivation failed: {e}")))?;
    Ok(key)
}

/// Generate `N` cryptographically secure random bytes.
pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    rand::rng().fill(&mut buf[..]);
    buf
}

/// Encode bytes as lowercase hex.
pub(crate) fn hex_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Decode a hex string to bytes.
pub(crate) fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 2 != 0 {
        return Err("hex string has odd length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("invalid hex: {e}")))
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let data = b"my secret config data";
        let password = "test-password-123";

        let encrypted = encrypt(data, password).unwrap();
        let text = std::str::from_utf8(&encrypted).unwrap();
        assert!(text.starts_with("DOTLING-ENC-V1\n"));

        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(&decrypted, data);
    }

    #[test]
    fn wrong_password_fails() {
        let data = b"secret";
        let encrypted = encrypt(data, "correct-password").unwrap();
        assert!(decrypt(&encrypted, "wrong-password").is_err());
    }

    #[test]
    fn hex_roundtrip() {
        let data = b"Hello, world!";
        let encoded = hex_encode(data);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(&decoded, data);
    }
}
