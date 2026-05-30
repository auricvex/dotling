use std::{fs, path::Path};

use crate::{
    config::{Config, Entry},
    error::{Error, Result},
    store, ui,
};

/// Encrypt tracked entries.
pub fn run_encrypt(paths: &[String]) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let mut config = Config::load(&config_path)?;

    let password = ui::password("Vault password");
    let master_key = crate::crypto::vault::unlock_vault(&password)?;
    let mut encrypted_count = 0usize;
    let mut errors = 0usize;

    for query in paths {
        let entry = config.find_entry_mut(query);
        match entry {
            Some(entry) if entry.encrypted => {
                ui::warning(&format!("`{}` is already encrypted", entry.source));
            }
            Some(entry) => match encrypt_single_entry(entry, &repo_root, &master_key) {
                Ok(true) => {
                    ui::success(&format!("encrypted `{}`", entry.source));
                    encrypted_count += 1;
                }
                Ok(false) => {}
                Err(e) => {
                    ui::error(&format!("{e}"));
                    errors += 1;
                }
            },
            None => {
                ui::error(&format!("`{query}` is not tracked"));
                errors += 1;
            }
        }
    }

    config.save()?;
    ui::summary(encrypted_count, 0, errors);

    Ok(())
}

/// Decrypt encrypted entries back to plaintext.
pub fn run_decrypt(paths: &[String]) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let mut config = Config::load(&config_path)?;

    let password = ui::password("Vault password");
    let master_key = crate::crypto::vault::unlock_vault(&password)?;
    let mut decrypted_count = 0usize;
    let mut errors = 0usize;

    for query in paths {
        let entry = config.find_entry_mut(query);
        match entry {
            Some(entry) if !entry.encrypted => {
                ui::warning(&format!("`{}` is not encrypted", entry.source));
            }
            Some(entry) => match decrypt_single_entry(entry, &repo_root, &master_key) {
                Ok(true) => {
                    ui::success(&format!("decrypted `{}`", entry.source));
                    decrypted_count += 1;
                }
                Ok(false) => {}
                Err(e) => {
                    ui::error(&format!("{e}"));
                    errors += 1;
                }
            },
            None => {
                ui::error(&format!("`{query}` is not tracked"));
                errors += 1;
            }
        }
    }

    config.save()?;
    ui::summary(decrypted_count, 0, errors);

    Ok(())
}

// ── Testable entry helpers ──────────────────────────────────────────

/// Encrypt a single entry. Returns `Ok(true)` if encrypted, `Ok(false)` if
/// already encrypted (no-op). For directory entries, delegates to
/// `encrypt_directory`.
pub fn encrypt_single_entry(entry: &mut Entry, repo_root: &Path, key: &[u8; 32]) -> Result<bool> {
    if entry.encrypted {
        return Ok(false);
    }

    let source_path = repo_root.join(&entry.source);

    if entry.directory {
        if !source_path.exists() {
            return Err(Error::io(
                &source_path,
                "read directory",
                std::io::Error::new(std::io::ErrorKind::NotFound, "source directory not found"),
            ));
        }
        encrypt_directory(&source_path, key)?;
        entry.encrypted = true;
        return Ok(true);
    }

    if !source_path.exists() {
        return Err(Error::io(
            &source_path,
            "read",
            std::io::Error::new(std::io::ErrorKind::NotFound, "source file not found"),
        ));
    }

    let content = fs::read(&source_path).map_err(|e| Error::io(&source_path, "read", e))?;
    let encrypted = crate::crypto::encrypt_with_key(&content, key)?;

    // Write encrypted content in-place
    crate::fs::atomic_write(&source_path, &encrypted)?;

    entry.encrypted = true;
    Ok(true)
}

/// Decrypt a single entry. Returns `Ok(true)` if decrypted, `Ok(false)` if
/// not encrypted (no-op). For directory entries, delegates to
/// `decrypt_directory`.
pub fn decrypt_single_entry(entry: &mut Entry, repo_root: &Path, key: &[u8; 32]) -> Result<bool> {
    if !entry.encrypted {
        return Ok(false);
    }

    if entry.directory {
        let source_path = repo_root.join(&entry.source);
        if !source_path.exists() {
            return Err(Error::io(
                &source_path,
                "read directory",
                std::io::Error::new(std::io::ErrorKind::NotFound, "source directory not found"),
            ));
        }
        decrypt_directory(&source_path, key)?;
        entry.encrypted = false;
        return Ok(true);
    }

    let source_path = repo_root.join(&entry.source);

    if !source_path.exists() {
        return Err(Error::io(
            &source_path,
            "read encrypted",
            std::io::Error::new(std::io::ErrorKind::NotFound, "encrypted source not found"),
        ));
    }

    let encrypted =
        fs::read(&source_path).map_err(|e| Error::io(&source_path, "read encrypted", e))?;
    let plaintext = crate::crypto::decrypt_with_key(&encrypted, key)?;

    crate::fs::atomic_write(&source_path, &plaintext)?;

    entry.encrypted = false;
    Ok(true)
}

// ── Directory helpers ─────────────────────────────────────────────

/// Recursively encrypt all plaintext files in a directory in-place.
///
/// Already-encrypted files are skipped.
/// Returns the number of files encrypted.
fn encrypt_directory(dir: &Path, key: &[u8; 32]) -> Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();

        if path.is_dir() {
            count += encrypt_directory(&path, key)?;
        } else {
            let content = fs::read(&path).map_err(|e| Error::io(&path, "read", e))?;
            if crate::crypto::is_encrypted_content(&content) {
                continue;
            }
            let encrypted = crate::crypto::encrypt_with_key(&content, key)?;
            crate::fs::atomic_write(&path, &encrypted)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Recursively decrypt all encrypted files in a directory in-place.
///
/// Returns the number of files decrypted.
fn decrypt_directory(dir: &Path, key: &[u8; 32]) -> Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();

        if path.is_dir() {
            count += decrypt_directory(&path, key)?;
        } else {
            let content = fs::read(&path).map_err(|e| Error::io(&path, "read", e))?;
            if !crate::crypto::is_encrypted_content(&content) {
                continue;
            }
            let plaintext = crate::crypto::decrypt_with_key(&content, key)?;
            crate::fs::atomic_write(&path, &plaintext)?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [0x42u8; 32]
    }

    fn make_entry(source: &str, target: &str, template: bool, encrypted: bool) -> Entry {
        Entry {
            source: source.into(),
            target: target.into(),
            method: None,
            encrypted,
            directory: false,
            template,
            os: None,
            permissions: None,
            before: None,
            after: None,
        }
    }

    fn make_dir_entry(source: &str, target: &str, encrypted: bool) -> Entry {
        Entry {
            source: source.into(),
            target: target.into(),
            method: None,
            encrypted,
            directory: true,
            template: false,
            os: None,
            permissions: None,
            before: None,
            after: None,
        }
    }

    // ── encrypt_directory tests ──────────────────────────────────

    #[test]
    fn encrypt_directory_single_file() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.txt"), "hello").unwrap();

        let count = encrypt_directory(&dir, &test_key()).unwrap();
        assert_eq!(count, 1);
        // File stays in-place with encrypted content
        assert!(dir.join("config.txt").exists());
        let enc = fs::read(dir.join("config.txt")).unwrap();
        assert!(crate::crypto::is_encrypted_content(&enc));
        let dec = crate::crypto::decrypt_with_key(&enc, &test_key()).unwrap();
        assert_eq!(dec, b"hello");
    }

    #[test]
    fn encrypt_directory_skips_existing_enc() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();
        // Write already-encrypted content (detected by header, not extension)
        let key = test_key();
        let already_enc = crate::crypto::encrypt_with_key(b"already encrypted", &key).unwrap();
        fs::write(dir.join("file.dat"), &already_enc).unwrap();
        fs::write(dir.join("plain.txt"), "plaintext").unwrap();

        let count = encrypt_directory(&dir, &key).unwrap();
        assert_eq!(count, 1); // only plain.txt
        assert_eq!(fs::read(dir.join("file.dat")).unwrap(), already_enc);
    }

    #[test]
    fn encrypt_directory_double_extension() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("foo.conf"), "data").unwrap();

        encrypt_directory(&dir, &test_key()).unwrap();
        // File stays in-place with encrypted content
        assert!(dir.join("foo.conf").exists());
        let enc = fs::read(dir.join("foo.conf")).unwrap();
        assert!(crate::crypto::is_encrypted_content(&enc));
    }

    #[test]
    fn encrypt_directory_nested() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), "aaa").unwrap();
        fs::write(dir.join("sub/b.txt"), "bbb").unwrap();

        let count = encrypt_directory(&dir, &test_key()).unwrap();
        assert_eq!(count, 2);
        // Files stay in-place with encrypted content
        assert!(crate::crypto::is_encrypted_content(
            &fs::read(dir.join("a.txt")).unwrap()
        ));
        assert!(crate::crypto::is_encrypted_content(
            &fs::read(dir.join("sub/b.txt")).unwrap()
        ));
    }

    #[test]
    fn encrypt_directory_empty_dir() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();

        let count = encrypt_directory(&dir, &test_key()).unwrap();
        assert_eq!(count, 0);
    }

    // ── decrypt_directory tests ──────────────────────────────────

    #[test]
    fn decrypt_directory_single_file() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();

        let key = test_key();
        let enc = crate::crypto::encrypt_with_key(b"hello", &key).unwrap();
        fs::write(dir.join("config.txt"), &enc).unwrap();

        let count = decrypt_directory(&dir, &key).unwrap();
        assert_eq!(count, 1);
        assert_eq!(fs::read(dir.join("config.txt")).unwrap(), b"hello");
    }

    #[test]
    fn decrypt_directory_double_extension() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();

        let key = test_key();
        let enc = crate::crypto::encrypt_with_key(b"data", &key).unwrap();
        fs::write(dir.join("foo.conf"), &enc).unwrap();

        decrypt_directory(&dir, &key).unwrap();
        assert_eq!(fs::read(dir.join("foo.conf")).unwrap(), b"data");
    }

    #[test]
    fn decrypt_directory_skips_non_enc() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();

        let key = test_key();
        fs::write(dir.join("plain.txt"), "untouched").unwrap();
        let enc = crate::crypto::encrypt_with_key(b"secret", &key).unwrap();
        fs::write(dir.join("secret.txt"), &enc).unwrap();

        let count = decrypt_directory(&dir, &key).unwrap();
        assert_eq!(count, 1);
        assert_eq!(fs::read(dir.join("plain.txt")).unwrap(), b"untouched");
        assert_eq!(fs::read(dir.join("secret.txt")).unwrap(), b"secret");
    }

    #[test]
    fn decrypt_directory_nested() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(dir.join("sub")).unwrap();

        let key = test_key();
        let enc1 = crate::crypto::encrypt_with_key(b"aaa", &key).unwrap();
        let enc2 = crate::crypto::encrypt_with_key(b"bbb", &key).unwrap();
        fs::write(dir.join("a.txt"), &enc1).unwrap();
        fs::write(dir.join("sub/b.txt"), &enc2).unwrap();

        let count = decrypt_directory(&dir, &key).unwrap();
        assert_eq!(count, 2);
        assert_eq!(fs::read(dir.join("a.txt")).unwrap(), b"aaa");
        assert_eq!(fs::read(dir.join("sub/b.txt")).unwrap(), b"bbb");
    }

    #[test]
    fn decrypt_directory_empty_dir() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("repo");
        fs::create_dir_all(&dir).unwrap();

        let count = decrypt_directory(&dir, &test_key()).unwrap();
        assert_eq!(count, 0);
    }

    // ── decrypt_single_entry tests ───────────────────────────────

    #[test]
    fn decrypt_template_enc_in_source() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key = test_key();
        let plaintext = b"# template\n{{ var.name }}";
        let encrypted = crate::crypto::encrypt_with_key(plaintext, &key).unwrap();

        // Encrypted content stored at the source path in-place
        let source_path = repo.join("shell/zshrc.dtmpl.enc");
        crate::fs::atomic_write(&source_path, &encrypted).unwrap();

        let mut entry = make_entry("shell/zshrc.dtmpl.enc", "~/.zshrc", true, true);

        let result = decrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(!entry.encrypted);

        // Decrypted in-place at the same path
        assert_eq!(fs::read(&source_path).unwrap(), plaintext);
    }

    #[test]
    fn decrypt_template_enc_not_in_source() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key = test_key();
        let plaintext = b"template content";
        let encrypted = crate::crypto::encrypt_with_key(plaintext, &key).unwrap();
        // Encrypted content stored at the source path in-place
        let source_path = repo.join("shell/zshrc.dtmpl");
        crate::fs::atomic_write(&source_path, &encrypted).unwrap();

        let mut entry = make_entry("shell/zshrc.dtmpl", "~/.zshrc", true, true);
        let result = decrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(!entry.encrypted);
        // Decrypted in-place
        assert_eq!(fs::read(&source_path).unwrap(), plaintext);
    }

    #[test]
    fn decrypt_plain_file() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key = test_key();
        let plaintext = b"zsh config";
        let encrypted = crate::crypto::encrypt_with_key(plaintext, &key).unwrap();
        // Encrypted content stored at the source path in-place
        let source_path = repo.join("shell/zshrc");
        crate::fs::atomic_write(&source_path, &encrypted).unwrap();

        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, true);
        let result = decrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(!entry.encrypted);
        // Decrypted in-place
        assert_eq!(fs::read(&source_path).unwrap(), plaintext);
    }

    #[test]
    fn decrypt_already_decrypted() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let key = test_key();
        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, false);
        let result = decrypt_single_entry(&mut entry, repo, &key).unwrap();
        assert!(!result);
    }

    #[test]
    fn decrypt_directory_entry() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let dir_path = repo.join("secrets");
        fs::create_dir_all(&dir_path).unwrap();

        let key = test_key();
        let enc = crate::crypto::encrypt_with_key(b"secret", &key).unwrap();
        // Encrypted content stored in-place
        fs::write(dir_path.join("key"), &enc).unwrap();

        let mut entry = make_dir_entry("secrets", "~/.secrets", true);
        let result = decrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(!entry.encrypted);
        assert_eq!(fs::read(dir_path.join("key")).unwrap(), b"secret");
    }

    #[test]
    fn decrypt_missing_enc_file() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        let key = test_key();
        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, true);
        let result = decrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_wrong_key() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key_a = [0x11u8; 32];
        let key_b = [0x22u8; 32];
        let encrypted = crate::crypto::encrypt_with_key(b"secret", &key_a).unwrap();
        // Encrypted content stored at the source path in-place
        crate::fs::atomic_write(&repo.join("shell/zshrc"), &encrypted).unwrap();

        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, true);
        let result = decrypt_single_entry(&mut entry, &repo, &key_b);
        assert!(result.is_err());
    }

    // ── encrypt_single_entry tests ───────────────────────────────

    #[test]
    fn encrypt_template_entry() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key = test_key();
        let plaintext = b"{{ var.name }}";
        fs::write(repo.join("shell/zshrc.dtmpl"), plaintext).unwrap();

        let mut entry = make_entry("shell/zshrc.dtmpl", "~/.zshrc", true, false);
        let result = encrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(entry.encrypted);
        // File stays in-place with encrypted content
        let enc = fs::read(repo.join("shell/zshrc.dtmpl")).unwrap();
        assert!(crate::crypto::is_encrypted_content(&enc));
        let dec = crate::crypto::decrypt_with_key(&enc, &key).unwrap();
        assert_eq!(dec, plaintext);
    }

    #[test]
    fn encrypt_plain_file() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key = test_key();
        fs::write(repo.join("shell/zshrc"), b"config").unwrap();

        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, false);
        let result = encrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(entry.encrypted);
        // File stays in-place with encrypted content
        let enc = fs::read(repo.join("shell/zshrc")).unwrap();
        assert!(crate::crypto::is_encrypted_content(&enc));
    }

    #[test]
    fn encrypt_already_encrypted() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let key = test_key();
        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, true);
        let result = encrypt_single_entry(&mut entry, repo, &key).unwrap();
        assert!(!result);
    }

    #[test]
    fn encrypt_directory_entry() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let dir_path = repo.join("secrets");
        fs::create_dir_all(&dir_path).unwrap();
        fs::write(dir_path.join("key.pem"), b"private").unwrap();

        let key = test_key();
        let mut entry = make_dir_entry("secrets", "~/.secrets", false);
        let result = encrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_ok());
        assert!(entry.encrypted);
        // File stays in-place with encrypted content
        let enc = fs::read(dir_path.join("key.pem")).unwrap();
        assert!(crate::crypto::is_encrypted_content(&enc));
    }

    #[test]
    fn encrypt_missing_source() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        let key = test_key();
        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, false);
        let result = encrypt_single_entry(&mut entry, &repo, &key);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_then_decrypt_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("shell")).unwrap();

        let key = test_key();
        let original = b"my dotfile content with special chars: !@#$%^&*()";

        // Create a plain file, encrypt it, then decrypt it
        fs::write(repo.join("shell/zshrc"), original).unwrap();
        let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, false);

        encrypt_single_entry(&mut entry, &repo, &key).unwrap();
        assert!(entry.encrypted);

        decrypt_single_entry(&mut entry, &repo, &key).unwrap();
        assert!(!entry.encrypted);

        let content = fs::read(repo.join("shell/zshrc")).unwrap();
        assert_eq!(content, original);
    }
}
