//! Secret vault management for dotling.
//!
//! Stores encrypted secrets at `~/.dotling/vault/`:
//! - `identity.enc` — password-encrypted secret material
//! - `config.toml`  — vault metadata (not secret)

use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::prelude::*;
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};

use super::{derive_key, hex_decode, hex_encode, random_bytes};
use crate::error::{Error, Result};

// ── Public API ────────────────────────────────────────────────────

/// Returns the vault directory path (`~/.dotling/vault/`).
pub fn vault_dir() -> Result<PathBuf> {
    let home = crate::path::home_dir()?;
    Ok(home.join(".dotling").join("vault"))
}

/// Returns `true` if the vault has been initialized.
pub fn vault_exists() -> bool {
    vault_dir()
        .is_ok_and(|dir| dir.join("identity.enc").exists() && dir.join("config.toml").exists())
}

/// Initialize a new vault.
///
/// Generates a random 32-byte secret, encrypts it with the user's password
/// via Argon2id + ChaCha20-Poly1305, and writes the vault files.
pub fn init_vault(password: &str) -> Result<()> {
    let dir = vault_dir()?;

    if dir.join("identity.enc").exists() {
        return Err(Error::Vault(
            "vault already exists — use change_password to update".into(),
        ));
    }

    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, "create vault directory", e))?;

    let secret: [u8; 32] = random_bytes();
    write_identity(&dir, password, &secret)?;
    write_config(&dir)?;

    Ok(())
}

/// Unlock the vault and return the decrypted secret material.
pub fn unlock_vault(password: &str) -> Result<[u8; 32]> {
    let dir = vault_dir()?;
    let identity_path = dir.join("identity.enc");

    if !identity_path.exists() {
        return Err(Error::Vault(
            "vault not initialized — run `dotling vault init` first".into(),
        ));
    }

    let content = fs::read_to_string(&identity_path)
        .map_err(|e| Error::io(&identity_path, "read vault identity", e))?;

    let decrypted = decrypt_identity(&content, password)?;
    decrypted
        .try_into()
        .map_err(|_| Error::Vault("corrupted vault secret length".into()))
}

/// Export the vault as a single encrypted bundle file at `path`.
pub fn export_vault(path: &Path, password: &str) -> Result<()> {
    let _ = unlock_vault(password)?; // verify password

    let dir = vault_dir()?;
    let identity = fs::read(dir.join("identity.enc"))
        .map_err(|e| Error::io(dir.join("identity.enc"), "read identity", e))?;
    let config = fs::read(dir.join("config.toml"))
        .map_err(|e| Error::io(dir.join("config.toml"), "read config", e))?;

    write_bundle(path, password, &config, &identity)
}

/// Import a vault bundle from `path`, decrypting with `password`.
pub fn import_vault(path: &Path, password: &str) -> Result<()> {
    let (config, identity) = read_bundle(path, password)?;

    let dir = vault_dir()?;
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, "create vault directory", e))?;

    crate::fs::atomic_write(&dir.join("identity.enc"), &identity)?;
    crate::fs::atomic_write(&dir.join("config.toml"), &config)?;

    // Verify the imported identity is valid
    let identity_str = std::str::from_utf8(&identity)
        .map_err(|_| Error::Vault("corrupted identity content in bundle".into()))?;
    let _ = decrypt_identity(identity_str, password)?;

    Ok(())
}

/// Change the vault password.
pub fn change_password(old_password: &str, new_password: &str) -> Result<()> {
    let secret = unlock_vault(old_password)?;
    let dir = vault_dir()?;
    write_identity(&dir, new_password, &secret)?;
    Ok(())
}

// ── Bundle helpers ───────────────────────────────────────────────

/// Encrypt and write a vault bundle to `path`.
fn write_bundle(path: &Path, password: &str, config: &[u8], identity: &[u8]) -> Result<()> {
    let salt: [u8; SALT_LEN] = random_bytes();
    let nonce_bytes: [u8; BUNDLE_NONCE_LEN] = random_bytes();

    // Build plaintext payload: [config_len:4] [config] [identity]
    let config_len =
        u32::try_from(config.len()).map_err(|_| Error::Vault("config.toml too large".into()))?;
    let mut payload = Vec::with_capacity(4 + config.len() + identity.len());
    payload.extend_from_slice(&config_len.to_be_bytes());
    payload.extend_from_slice(config);
    payload.extend_from_slice(identity);

    let key = derive_key(password, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, payload.as_ref())
        .map_err(|e| Error::Vault(format!("bundle encryption failed: {e}")))?;

    // Write: magic | version | salt | nonce | ciphertext+tag
    let mut blob = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    blob.extend_from_slice(BUNDLE_MAGIC);
    blob.push(BUNDLE_VERSION);
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);

    crate::fs::atomic_write(path, &blob)
}

/// Read and decrypt a vault bundle from `path`.
///
/// Returns `(config_content, identity_content)`.
fn read_bundle(path: &Path, password: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let blob = fs::read(path).map_err(|e| Error::io(path, "read vault bundle", e))?;

    let min_size = HEADER_LEN + TAG_LEN + 4;
    if blob.len() < min_size {
        return Err(Error::Vault(format!(
            "bundle too small ({} bytes, expected at least {min_size})",
            blob.len()
        )));
    }

    if &blob[..8] != BUNDLE_MAGIC {
        return Err(Error::Vault(
            "invalid bundle format (bad magic bytes)".into(),
        ));
    }
    if blob[8] != BUNDLE_VERSION {
        return Err(Error::Vault(format!(
            "unsupported bundle version: {}",
            blob[8]
        )));
    }

    let salt = &blob[9..9 + SALT_LEN];
    let nonce_bytes = &blob[9 + SALT_LEN..9 + SALT_LEN + BUNDLE_NONCE_LEN];
    let ciphertext = &blob[9 + SALT_LEN + BUNDLE_NONCE_LEN..];

    let key = derive_key(password, salt)?;
    let cipher = ChaCha20Poly1305::new(&key.into());
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| Error::Vault("incorrect password or corrupted bundle".into()))?;

    if plaintext.len() < 4 {
        return Err(Error::Vault("bundle payload too short".into()));
    }

    let config_len = u32::from_be_bytes(plaintext[..4].try_into().unwrap()) as usize;
    if config_len > plaintext.len() - 4 {
        return Err(Error::Vault("bundle payload truncated".into()));
    }

    let config = plaintext[4..4 + config_len].to_vec();
    let identity = plaintext[4 + config_len..].to_vec();

    Ok((config, identity))
}

// ── Internal helpers ──────────────────────────────────────────────

const VAULT_HEADER: &str = "DOTLING-VAULT-V1";

// Bundle format: single encrypted file for vault export/import.
const BUNDLE_MAGIC: &[u8; 8] = b"DOTLVAUL";
const BUNDLE_VERSION: u8 = 0x01;
const SALT_LEN: usize = 32;
const BUNDLE_NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16; // ChaCha20-Poly1305 AEAD tag
const HEADER_LEN: usize = 8 + 1 + SALT_LEN + BUNDLE_NONCE_LEN; // 53

/// Encrypt `secret` with `password` and write `identity.enc`.
fn write_identity(dir: &Path, password: &str, secret: &[u8]) -> Result<()> {
    let salt: [u8; 32] = random_bytes();
    let nonce_bytes: [u8; 12] = random_bytes();

    let key = derive_key(password, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, secret)
        .map_err(|e| Error::Vault(format!("vault encryption failed: {e}")))?;

    let content = format!(
        "{VAULT_HEADER}\n{}\n{}\n{}\n",
        hex_encode(&salt),
        hex_encode(&nonce_bytes),
        BASE64_STANDARD.encode(&encrypted),
    );

    let path = dir.join("identity.enc");
    crate::fs::atomic_write(&path, content.as_bytes())
}

/// Write `config.toml` with vault metadata.
fn write_config(dir: &Path) -> Result<()> {
    let now = current_timestamp();
    let content = format!("[vault]\nversion = 1\ncreated = \"{now}\"\n");
    let path = dir.join("config.toml");
    crate::fs::atomic_write(&path, content.as_bytes())
}

/// Parse and decrypt an `identity.enc` file.
fn decrypt_identity(content: &str, password: &str) -> Result<Vec<u8>> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() < 4 || lines[0] != VAULT_HEADER {
        return Err(Error::Vault("invalid vault identity format".into()));
    }

    let salt = hex_decode(lines[1]).map_err(|e| Error::Vault(format!("invalid salt hex: {e}")))?;
    let nonce_bytes =
        hex_decode(lines[2]).map_err(|e| Error::Vault(format!("invalid nonce hex: {e}")))?;
    let encrypted = BASE64_STANDARD
        .decode(lines[3])
        .map_err(|e| Error::Vault(format!("invalid base64 payload: {e}")))?;

    if salt.len() != 32 {
        return Err(Error::Vault(format!(
            "expected 32-byte salt, got {}",
            salt.len()
        )));
    }
    if nonce_bytes.len() != 12 {
        return Err(Error::Vault(format!(
            "expected 12-byte nonce, got {}",
            nonce_bytes.len()
        )));
    }

    let key = derive_key(password, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key.into());
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, encrypted.as_ref())
        .map_err(|_| Error::Vault("incorrect password or corrupted vault".into()))
}

/// Simple UTC timestamp without chrono.
fn current_timestamp() -> String {
    use std::time::SystemTime;

    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = secs / 86400;
    let tod = secs % 86400;
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    days += 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a closure with HOME set to a temporary directory, then restore it.
    fn with_temphome(f: impl FnOnce(&Path)) {
        let _guard = crate::core::ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp = tempfile::tempdir().unwrap();
        let oldhome = std::env::var_os("HOME");
        // SAFETY: serialized via HOME_LOCK
        unsafe {
            std::env::set_var("HOME", temp.path());
        }
        f(temp.path());
        match oldhome {
            Some(h) => unsafe {
                std::env::set_var("HOME", h);
            },
            None => unsafe {
                std::env::remove_var("HOME");
            },
        }
    }

    #[test]
    fn vault_dir_is_underhome() {
        let dir = vault_dir().unwrap();
        assert!(dir.ends_with(".dotling/vault"));
    }

    #[test]
    fn days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        assert_eq!(days_to_ymd(19723), (2024, 1, 1));
    }

    // ── write_identity / decrypt_identity roundtrip ────────────

    #[test]
    fn write_identity_decrypt_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let secret = [0x42u8; 32];
        write_identity(temp.path(), "test-pass", &secret).unwrap();

        let content = fs::read_to_string(temp.path().join("identity.enc")).unwrap();
        let decrypted = decrypt_identity(&content, "test-pass").unwrap();
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn decrypt_identity_wrong_password() {
        let temp = tempfile::tempdir().unwrap();
        let secret = [0x42u8; 32];
        write_identity(temp.path(), "correct", &secret).unwrap();

        let content = fs::read_to_string(temp.path().join("identity.enc")).unwrap();
        let result = decrypt_identity(&content, "wrong");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_identity_corrupted_header() {
        let result = decrypt_identity("WRONG-HEADER\nabc\nabc\nabc\n", "pass");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid vault"));
    }

    #[test]
    fn decrypt_identity_truncated() {
        let result = decrypt_identity("DOTLING-VAULT-V1\n", "pass");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_identity_bad_salt_length() {
        let content = "DOTLING-VAULT-V1\nabcd\nabcd1234abcd1234abcd1234\ndGVzdA==\n";
        let result = decrypt_identity(content, "pass");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("salt"));
    }

    #[test]
    fn decrypt_identity_bad_nonce_length() {
        let salt = hex_encode(&[0u8; 32]);
        let content = format!("DOTLING-VAULT-V1\n{salt}\nabcd\ndGVzdA==\n");
        let result = decrypt_identity(&content, "pass");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nonce"));
    }

    // ── Public API tests (require HOME override) ───────────────
    //
    // All HOME-mutating tests are combined into a single test function
    // to avoid interference from parallel tests that also mutate HOME
    // (e.g. commands/remove.rs).

    #[test]
    fn vault_public_api() {
        with_temphome(|home| {
            // vault_exists before init
            assert!(!vault_exists());

            // unlock before init errors
            let result = unlock_vault("password");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not initialized"));

            // init creates files
            init_vault("password").unwrap();
            let dir = vault_dir().unwrap();
            assert!(dir.join("identity.enc").exists());
            assert!(dir.join("config.toml").exists());
            assert!(vault_exists());

            // init twice errors
            let result = init_vault("password");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already exists"));

            // unlock with correct password
            let key = unlock_vault("password").unwrap();
            assert_eq!(key.len(), 32);

            // unlock with wrong password
            assert!(unlock_vault("wrong").is_err());

            // change password
            change_password("password", "new-pass").unwrap();
            assert!(unlock_vault("password").is_err());
            let key2 = unlock_vault("new-pass").unwrap();
            assert_eq!(key2.len(), 32);

            // export and import (single-file bundle)
            let bundle_path = home.join("vault-backup.bin");
            export_vault(&bundle_path, "new-pass").unwrap();
            assert!(bundle_path.exists());
            assert!(fs::metadata(&bundle_path).unwrap().len() > HEADER_LEN as u64);

            fs::remove_dir_all(&dir).unwrap();
            import_vault(&bundle_path, "new-pass").unwrap();
            let restored = unlock_vault("new-pass").unwrap();
            assert_eq!(key2, restored);

            // import with wrong password fails
            let bundle2 = home.join("vault-backup2.bin");
            export_vault(&bundle2, "new-pass").unwrap();
            fs::remove_dir_all(&dir).unwrap();
            assert!(import_vault(&bundle2, "wrong-password").is_err());

            // bundle has correct magic bytes
            let raw = fs::read(&bundle2).unwrap();
            assert_eq!(&raw[..8], b"DOTLVAUL");
            assert_eq!(raw[8], 0x01);

            // truncated bundle is rejected
            let truncated = home.join("truncated.bin");
            fs::write(&truncated, b"DOTLVAUL\x01").unwrap();
            assert!(import_vault(&truncated, "any").is_err());

            // wrong magic is rejected
            let bad_magic = home.join("bad-magic.bin");
            let mut bad_raw = raw.clone();
            bad_raw[..8].copy_from_slice(b"WRONGMAG");
            fs::write(&bad_magic, &bad_raw).unwrap();
            assert!(import_vault(&bad_magic, "new-pass").is_err());
        });
    }
}
