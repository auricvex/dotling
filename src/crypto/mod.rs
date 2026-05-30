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

const HEADER: &str = "DOTLING-ENC-V2";
const NONCE_LEN: usize = 12;

// ── High-level encrypt / decrypt API ──────────────────────────────

/// Encrypt data with a 32-byte symmetric key.
///
/// Output format (text):
/// ```text
/// DOTLING-ENC-V2
/// <12-byte nonce as hex>
/// <base64-encoded ciphertext+tag>
/// ```
pub fn encrypt_with_key(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let nonce_bytes = random_bytes::<NONCE_LEN>();

    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| Error::Crypto(format!("encryption failed: {e}")))?;

    let output = format!(
        "{HEADER}\n{}\n{}\n",
        hex_encode(&nonce_bytes),
        BASE64_STANDARD.encode(&ciphertext),
    );

    Ok(output.into_bytes())
}

/// Decrypt data with a 32-byte symmetric key.
///
/// Expects the `DOTLING-ENC-V2` format produced by [`encrypt_with_key`].
pub fn decrypt_with_key(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(data)
        .map_err(|_| Error::Crypto("invalid UTF-8 in encrypted file".into()))?;

    let lines: Vec<&str> = text.lines().collect();

    if lines.len() < 3 || lines[0] != HEADER {
        return Err(Error::Crypto(
            "not a valid dotling encrypted file (expected DOTLING-ENC-V2 header)".into(),
        ));
    }

    let nonce_bytes =
        hex_decode(lines[1]).map_err(|e| Error::Crypto(format!("invalid nonce: {e}")))?;
    let ciphertext = BASE64_STANDARD
        .decode(lines[2])
        .map_err(|e| Error::Crypto(format!("invalid payload: {e}")))?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(Error::Crypto(format!(
            "expected {NONCE_LEN}-byte nonce, got {}",
            nonce_bytes.len()
        )));
    }

    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| Error::Crypto("decryption failed — wrong key or corrupted data".into()))
}

/// Returns `true` if `data` starts with the `DOTLING-ENC-V2` header.
pub fn is_encrypted_content(data: &[u8]) -> bool {
    data.starts_with(HEADER.as_bytes()) && data.get(HEADER.len()) == Some(&b'\n')
}

// ── Internal helpers ──────────────────────────────────────────────

/// Derive a 32-byte key from a password + salt using Argon2id.
pub(crate) fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
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
        let key = [0x42u8; 32];

        let encrypted = encrypt_with_key(data, &key).unwrap();
        let text = std::str::from_utf8(&encrypted).unwrap();
        assert!(text.starts_with("DOTLING-ENC-V2\n"));

        let decrypted = decrypt_with_key(&encrypted, &key).unwrap();
        assert_eq!(&decrypted, data);
    }

    #[test]
    fn wrong_key_fails() {
        let data = b"secret";
        let key1 = [0x11u8; 32];
        let key2 = [0x22u8; 32];
        let encrypted = encrypt_with_key(data, &key1).unwrap();
        assert!(decrypt_with_key(&encrypted, &key2).is_err());
    }

    #[test]
    fn hex_roundtrip() {
        let data = b"Hello, world!";
        let encoded = hex_encode(data);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(&decoded, data);
    }

    // ── encrypt/decrypt edge cases ──────────────────────────────

    #[test]
    fn encrypt_decrypt_empty_data() {
        let key = [0x01u8; 32];
        let encrypted = encrypt_with_key(b"", &key).unwrap();
        let decrypted = decrypt_with_key(&encrypted, &key).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn encrypt_decrypt_large_data() {
        let key = [0x02u8; 32];
        let data = vec![0xABu8; 1024 * 1024]; // 1 MB
        let encrypted = encrypt_with_key(&data, &key).unwrap();
        let decrypted = decrypt_with_key(&encrypted, &key).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn encrypted_output_has_correct_header() {
        let key = [0x03u8; 32];
        let encrypted = encrypt_with_key(b"test", &key).unwrap();
        let text = std::str::from_utf8(&encrypted).unwrap();
        assert!(text.starts_with("DOTLING-ENC-V2\n"));
    }

    #[test]
    fn encrypted_output_is_valid_utf8() {
        let key = [0x04u8; 32];
        let encrypted = encrypt_with_key(b"binary-safe test", &key).unwrap();
        assert!(std::str::from_utf8(&encrypted).is_ok());
    }

    #[test]
    fn decrypt_invalid_header() {
        let key = [0x05u8; 32];
        let bad = b"WRONG-HEADER\nabc\nabc\n";
        let result = decrypt_with_key(bad, &key);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("DOTLING-ENC-V2"));
    }

    #[test]
    fn decrypt_truncated_data() {
        let key = [0x06u8; 32];
        let bad = b"DOTLING-ENC-V2\nabcd1234abcd1234abcd1234\n";
        let result = decrypt_with_key(bad, &key);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_invalid_base64() {
        let key = [0x07u8; 32];
        let bad = b"DOTLING-ENC-V2\nabcd1234abcd1234abcd1234\n!!!not-base64!!!\n";
        let result = decrypt_with_key(bad, &key);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_wrong_nonce_length() {
        let key = [0x08u8; 32];
        // 4 hex chars = 2 bytes, not 12
        let bad = b"DOTLING-ENC-V2\nabcd\ndGVzdA==\n";
        let result = decrypt_with_key(bad, &key);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nonce"));
    }

    #[test]
    fn decrypt_tampered_ciphertext() {
        let key = [0x09u8; 32];
        let encrypted = encrypt_with_key(b"original", &key).unwrap();
        let text = std::str::from_utf8(&encrypted).unwrap().to_string();

        // Tamper with the base64 payload: flip a character
        let mut lines: Vec<&str> = text.lines().collect();
        let payload = lines[2].as_bytes().to_vec();
        let mut tampered_payload = payload.clone();
        tampered_payload[0] = if tampered_payload[0] == b'A' {
            b'B'
        } else {
            b'A'
        };
        lines[2] = std::str::from_utf8(&tampered_payload).unwrap();

        let tampered = lines.join("\n");
        let result = decrypt_with_key(tampered.as_bytes(), &key);
        assert!(result.is_err());
    }

    // ── derive_key tests ────────────────────────────────────────

    #[test]
    fn derive_key_deterministic() {
        let password = "test-password";
        let salt = b"fixed-salt-value-1234567890ab";
        let key1 = derive_key(password, salt).unwrap();
        let key2 = derive_key(password, salt).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn derive_key_different_salt() {
        let password = "test-password";
        let salt1 = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let salt2 = b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let key1 = derive_key(password, salt1).unwrap();
        let key2 = derive_key(password, salt2).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn derive_key_empty_password() {
        let salt = b"some-salt-value-1234567890abcd";
        let key = derive_key("", salt).unwrap();
        assert_eq!(key.len(), 32);
    }

    // ── random_bytes tests ──────────────────────────────────────

    #[test]
    fn random_bytes_different() {
        let a: [u8; 32] = random_bytes();
        let b: [u8; 32] = random_bytes();
        assert_ne!(a, b);
    }

    #[test]
    fn random_bytes_correct_length() {
        let a: [u8; 12] = random_bytes();
        let b: [u8; 32] = random_bytes();
        assert_eq!(a.len(), 12);
        assert_eq!(b.len(), 32);
    }

    // ── hex edge cases ──────────────────────────────────────────

    #[test]
    fn hex_decode_odd_length() {
        let result = hex_decode("abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("odd length"));
    }

    #[test]
    fn hex_decode_invalid_chars() {
        let result = hex_decode("zzzz");
        assert!(result.is_err());
    }

    #[test]
    fn hex_decode_empty() {
        let result = hex_decode("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn hex_encode_empty() {
        let result = hex_encode(b"");
        assert!(result.is_empty());
    }

    // ── is_encrypted_content tests ──────────────────────────────

    #[test]
    fn is_encrypted_content_detects_header() {
        let key = [0x42u8; 32];
        let encrypted = encrypt_with_key(b"secret", &key).unwrap();
        assert!(is_encrypted_content(&encrypted));
    }

    #[test]
    fn is_encrypted_content_rejects_plaintext() {
        assert!(!is_encrypted_content(b"just plain text"));
    }

    #[test]
    fn is_encrypted_content_rejects_partial_header() {
        assert!(!is_encrypted_content(b"DOTLING-ENC-V"));
    }

    #[test]
    fn is_encrypted_content_rejects_empty() {
        assert!(!is_encrypted_content(b""));
    }
}
