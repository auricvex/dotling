use std::{fs, path::Path};

use crate::{
    config::{DeployMethod, Entry},
    error::{Error, Result},
};

/// The observed state of a deployed entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    /// Correctly deployed and up-to-date.
    Deployed,
    /// Deployed but the target file has been modified (copy mode).
    Modified,
    /// Not deployed — target does not exist.
    Missing,
    /// Symlink exists but points to wrong target or is broken.
    Broken,
    /// An unmanaged file exists at the target path.
    Conflict,
}

/// Check the deployment state of an entry.
pub fn check_state(entry: &Entry, repo_root: &Path, default_method: DeployMethod) -> EntryState {
    let Ok(target) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) else {
        return EntryState::Missing;
    };

    // Templates: source is the .dtmpl file itself (always present by name),
    // and deployment is always copy mode (rendered output).
    // Encrypted templates: source is the .dtmpl.enc file.
    // Normal encrypted files: source is <name>.enc.
    let source = if entry.template {
        // The source field already contains the full name (e.g. "config/foo.dtmpl"
        // or "config/foo.dtmpl.enc") — use it as-is.
        repo_root.join(&entry.source)
    } else if entry.encrypted && !entry.directory {
        repo_root.join(format!("{}.enc", entry.source))
    } else {
        repo_root.join(&entry.source)
    };

    if !target.exists() && !crate::fs::is_symlink(&target) {
        return EntryState::Missing;
    }

    // Templates are always copy-deployed (rendered output is a plain file).
    let method = if entry.template {
        DeployMethod::Copy
    } else {
        entry.method.unwrap_or(default_method)
    };

    match method {
        DeployMethod::Symlink if !entry.encrypted => {
            if crate::fs::is_symlink(&target) {
                match crate::fs::read_link(&target) {
                    Ok(link_target) => {
                        if link_target == source {
                            EntryState::Deployed
                        } else {
                            EntryState::Broken
                        }
                    }
                    Err(_) => EntryState::Broken,
                }
            } else {
                // A regular file exists where we want a symlink
                EntryState::Conflict
            }
        }
        _ => {
            // Copy mode (or encrypted, or template — all use copy)
            if crate::fs::is_symlink(&target) {
                EntryState::Conflict
            } else if entry.encrypted && !entry.template {
                // For encrypted files, we can't easily check if content matches
                // without decrypting. Just check existence.
                EntryState::Deployed
            } else {
                // For templates, the "source" for comparison is the deployed rendered file,
                // not the .dtmpl file. We only need to know if the target exists.
                // Fingerprint-based comparison happens in status/sync.
                if entry.template {
                    EntryState::Deployed
                } else {
                    match crate::fs::files_identical(&source, &target) {
                        Ok(true) => EntryState::Deployed,
                        Ok(false) => EntryState::Modified,
                        Err(_) => EntryState::Broken,
                    }
                }
            }
        }
    }
}

/// Deploy a single entry (non-encrypted).
pub fn deploy_entry(
    entry: &Entry,
    repo_root: &Path,
    default_method: DeployMethod,
    force: bool,
) -> Result<()> {
    let method = entry.method.unwrap_or(default_method);
    let target = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;
    let source = repo_root.join(&entry.source);

    if !source.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!("source `{}` does not exist in repo", source.display()),
        });
    }

    // Handle existing target
    if target.exists() || crate::fs::is_symlink(&target) {
        if !force {
            let state = check_state(entry, repo_root, default_method);
            if state == EntryState::Deployed {
                return Ok(()); // Already deployed correctly
            }
            if state == EntryState::Conflict {
                return Err(Error::Deploy {
                    entry: entry.source.clone(),
                    message: format!(
                        "unmanaged file exists at `{}` — use --force to overwrite",
                        target.display()
                    ),
                });
            }
        }
        // Remove existing
        if crate::fs::is_symlink(&target) {
            crate::fs::remove_symlink(&target)?;
        } else if target.is_dir() {
            fs::remove_dir_all(&target).map_err(|e| Error::io(&target, "remove directory", e))?;
        } else {
            fs::remove_file(&target).map_err(|e| Error::io(&target, "remove file", e))?;
        }
    }

    match method {
        DeployMethod::Symlink => {
            crate::fs::create_symlink(&source, &target)?;
        }
        DeployMethod::Copy => {
            if entry.directory {
                copy_directory(&source, &target)?;
            } else {
                crate::fs::copy_file(&source, &target)?;
            }
        }
    }

    if let Some(perms) = entry.permissions {
        crate::fs::set_permissions(&target, perms)?;
    }

    Ok(())
}

/// Deploy an encrypted entry.
///
/// Reads the encrypted source from the repo, decrypts it using the provided
/// password, and writes the plaintext to the target.
pub fn deploy_encrypted(entry: &Entry, repo_root: &Path, password: &str) -> Result<()> {
    let target = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;
    let master_key = crate::crypto::vault::unlock_vault(password)?;

    if entry.directory {
        let source = repo_root.join(&entry.source);
        if !source.exists() {
            return Err(Error::Deploy {
                entry: entry.source.clone(),
                message: format!(
                    "encrypted source directory `{}` not found",
                    source.display()
                ),
            });
        }
        deploy_encrypted_directory(&source, &target, &master_key)?;
    } else {
        let source = repo_root.join(format!("{}.enc", entry.source));
        if !source.exists() {
            return Err(Error::Deploy {
                entry: entry.source.clone(),
                message: format!("encrypted source `{}` not found", source.display()),
            });
        }
        let encrypted =
            fs::read(&source).map_err(|e| Error::io(&source, "read encrypted file", e))?;
        let plaintext = crate::crypto::decrypt_with_key(&encrypted, &master_key)?;
        crate::fs::atomic_write(&target, &plaintext)?;
    }

    // Set restrictive permissions on decrypted files (Unix)
    if let Some(perms) = entry.permissions {
        crate::fs::set_permissions(&target, perms)?;
    } else if !entry.directory {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(&target, perms)
                .map_err(|e| Error::io(&target, "set permissions", e))?;
        }
    }

    Ok(())
}

fn deploy_encrypted_directory(src: &Path, dst: &Path, key: &[u8; 32]) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;

    for entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(src, "read directory entry", e))?;
        let src_path = entry.path();
        let file_name = entry.file_name();

        if src_path.is_dir() {
            let dst_path = dst.join(&file_name);
            deploy_encrypted_directory(&src_path, &dst_path, key)?;
        } else if src_path.extension().and_then(|e| e.to_str()) == Some("enc") {
            let encrypted =
                fs::read(&src_path).map_err(|e| Error::io(&src_path, "read encrypted file", e))?;
            let plaintext = crate::crypto::decrypt_with_key(&encrypted, key)?;

            let dst_name = Path::new(&file_name).with_extension("");
            let dst_path = dst.join(dst_name);
            crate::fs::atomic_write(&dst_path, &plaintext)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                fs::set_permissions(&dst_path, perms).ok();
            }
        } else {
            let dst_path = dst.join(&file_name);
            crate::fs::copy_file(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Recursively copy a directory.
fn copy_directory(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;

    for entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(src, "read directory entry", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_directory(&src_path, &dst_path)?;
        } else {
            crate::fs::copy_file(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
