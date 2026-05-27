use std::fs;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::{store, ui};

/// Encrypt tracked entries.
pub fn run_encrypt(paths: &[String]) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let mut config = Config::load(&config_path)?;

    let password = ui::password("Vault password");
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

                if !source_path.exists() {
                    ui::error(&format!(
                        "source `{}` not found in repo",
                        source_path.display()
                    ));
                    errors += 1;
                    continue;
                }

                // Read plaintext, encrypt, write .enc, remove original
                let content = fs::read(&source_path)
                    .map_err(|e| Error::io(&source_path, "read", e))?;

                let encrypted = crate::crypto::encrypt(&content, &password)?;
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
    let mut decrypted_count = 0usize;
    let mut errors = 0usize;

    for query in paths {
        let entry = config.find_entry_mut(query);
        match entry {
            Some(entry) if !entry.encrypted => {
                ui::warning(&format!("`{}` is not encrypted", entry.source));
            }
            Some(entry) => {
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
                let encrypted = fs::read(&enc_path)
                    .map_err(|e| Error::io(&enc_path, "read encrypted", e))?;

                let plaintext = crate::crypto::decrypt(&encrypted, &password)?;
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
