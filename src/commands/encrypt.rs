use std::{fs, path::Path};

use crate::{
    config::Config,
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
            Some(entry) => {
                let source_path = repo_root.join(&entry.source);

                if entry.directory {
                    // Encrypt every file inside the directory
                    if !source_path.exists() {
                        ui::error(&format!(
                            "source directory `{}` not found in repo",
                            source_path.display()
                        ));
                        errors += 1;
                        continue;
                    }

                    match encrypt_directory(&source_path, &master_key) {
                        Ok(n) => {
                            entry.encrypted = true;
                            ui::success(&format!(
                                "encrypted `{}` ({n} file{})",
                                entry.source,
                                if n == 1 { "" } else { "s" }
                            ));
                            encrypted_count += 1;
                        }
                        Err(e) => {
                            ui::error(&format!("failed to encrypt `{}`: {e}", entry.source));
                            errors += 1;
                        }
                    }
                    continue;
                }

                if !source_path.exists() {
                    ui::error(&format!(
                        "source `{}` not found in repo",
                        source_path.display()
                    ));
                    errors += 1;
                    continue;
                }

                // Read plaintext, encrypt, write .enc, remove original
                let content =
                    fs::read(&source_path).map_err(|e| Error::io(&source_path, "read", e))?;

                let encrypted = crate::crypto::encrypt_with_key(&content, &master_key)?;
                let enc_path = repo_root.join(format!("{}.enc", entry.source));
                crate::fs::atomic_write(&enc_path, &encrypted)?;

                // Remove plaintext from repo
                fs::remove_file(&source_path)
                    .map_err(|e| Error::io(&source_path, "remove plaintext", e))?;

                entry.encrypted = true;
                ui::success(&format!("encrypted `{}`", entry.source));
                encrypted_count += 1;
            }
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
            Some(entry) => {
                if entry.directory {
                    // Decrypt every .enc file inside the directory
                    let source_path = repo_root.join(&entry.source);
                    if !source_path.exists() {
                        ui::error(&format!(
                            "source directory `{}` not found in repo",
                            source_path.display()
                        ));
                        errors += 1;
                        continue;
                    }

                    match decrypt_directory(&source_path, &master_key) {
                        Ok(n) => {
                            entry.encrypted = false;
                            ui::success(&format!(
                                "decrypted `{}` ({n} file{})",
                                entry.source,
                                if n == 1 { "" } else { "s" }
                            ));
                            decrypted_count += 1;
                        }
                        Err(e) => {
                            ui::error(&format!("failed to decrypt `{}`: {e}", entry.source));
                            errors += 1;
                        }
                    }
                    continue;
                }

                let enc_path = repo_root.join(format!("{}.enc", entry.source));

                if !enc_path.exists() {
                    ui::error(&format!(
                        "encrypted source `{}` not found",
                        enc_path.display()
                    ));
                    errors += 1;
                    continue;
                }

                // Read encrypted, decrypt, write plaintext, remove .enc
                let encrypted =
                    fs::read(&enc_path).map_err(|e| Error::io(&enc_path, "read encrypted", e))?;

                let plaintext = crate::crypto::decrypt_with_key(&encrypted, &master_key)?;
                let source_path = repo_root.join(&entry.source);
                crate::fs::atomic_write(&source_path, &plaintext)?;

                // Remove encrypted file
                fs::remove_file(&enc_path)
                    .map_err(|e| Error::io(&enc_path, "remove encrypted", e))?;

                entry.encrypted = false;
                ui::success(&format!("decrypted `{}`", entry.source));
                decrypted_count += 1;
            }
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

// ── Directory helpers ─────────────────────────────────────────────

/// Recursively encrypt all plaintext files in a directory.
///
/// Each file `foo` becomes `foo.enc`; the original is removed.
/// Already-encrypted `.enc` files are skipped.
/// Returns the number of files encrypted.
fn encrypt_directory(dir: &Path, key: &[u8; 32]) -> Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();

        if path.is_dir() {
            count += encrypt_directory(&path, key)?;
        } else {
            // Skip files that are already encrypted
            if path.extension().and_then(|e| e.to_str()) == Some("enc") {
                continue;
            }

            let content = fs::read(&path).map_err(|e| Error::io(&path, "read", e))?;
            let encrypted = crate::crypto::encrypt_with_key(&content, key)?;

            let enc_path = path.with_extension(match path.extension().and_then(|e| e.to_str()) {
                Some(ext) => format!("{ext}.enc"),
                None => "enc".to_string(),
            });
            crate::fs::atomic_write(&enc_path, &encrypted)?;
            fs::remove_file(&path).map_err(|e| Error::io(&path, "remove plaintext", e))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Recursively decrypt all `.enc` files in a directory.
///
/// Each `foo.enc` is decrypted back to `foo`; the `.enc` file is removed.
/// Returns the number of files decrypted.
fn decrypt_directory(dir: &Path, key: &[u8; 32]) -> Result<usize> {
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();

        if path.is_dir() {
            count += decrypt_directory(&path, key)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("enc") {
            let encrypted = fs::read(&path).map_err(|e| Error::io(&path, "read encrypted", e))?;
            let plaintext = crate::crypto::decrypt_with_key(&encrypted, key)?;

            // Strip the `.enc` extension to get the original path
            let original_path = path.with_extension("");
            // Handle double extensions like `foo.conf.enc` → `foo.conf`
            crate::fs::atomic_write(&original_path, &plaintext)?;
            fs::remove_file(&path).map_err(|e| Error::io(&path, "remove encrypted", e))?;
            count += 1;
        }
    }
    Ok(count)
}
