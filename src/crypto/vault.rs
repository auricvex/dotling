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
pub fn unlock_vault(password: &str) -> Result<Vec<u8>> {
    let dir = vault_dir()?;
    let identity_path = dir.join("identity.enc");

    if !identity_path.exists() {
        return Err(Error::Vault(
            "vault not initialized — run `dotling vault init` first".into(),
        ));
    }

    let content = fs::read_to_string(&identity_path)
        .map_err(|e| Error::io(&identity_path, "read vault identity", e))?;

    decrypt_identity(&content, password)
}

/// Export the vault directory to a bundle at `path`.
pub fn export_vault(path: &Path, password: &str) -> Result<()> {
    let _ = unlock_vault(password)?; // verify password

    let dir = vault_dir()?;
    fs::create_dir_all(path).map_err(|e| Error::io(path, "create export directory", e))?;

    for name in &["identity.enc", "config.toml"] {
        let src = dir.join(name);
        let dst = path.join(name);
        if src.exists() {
            fs::copy(&src, &dst).map_err(|e| Error::io(&src, "copy vault file", e))?;
        }
    }

    Ok(())
}

/// Import a vault bundle from `path`.
pub fn import_vault(path: &Path) -> Result<()> {
    let identity_src = path.join("identity.enc");
    if !identity_src.exists() {
        return Err(Error::Vault(format!(
            "no identity.enc found in `{}`",
            path.display()
        )));
    }

    let dir = vault_dir()?;
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, "create vault directory", e))?;

    for name in &["identity.enc", "config.toml"] {
        let src = path.join(name);
        let dst = dir.join(name);
        if src.exists() {
            fs::copy(&src, &dst).map_err(|e| Error::io(&src, "import vault file", e))?;
        }
    }

    Ok(())
}

/// Change the vault password.
pub fn change_password(old_password: &str, new_password: &str) -> Result<()> {
    let secret = unlock_vault(old_password)?;
    let dir = vault_dir()?;
    write_identity(&dir, new_password, &secret)?;
    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────

const VAULT_HEADER: &str = "DOTLING-VAULT-V1";

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

    #[test]
    fn vault_dir_is_under_home() {
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
}
