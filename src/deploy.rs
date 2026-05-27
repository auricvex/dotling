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
    let method = entry.method.unwrap_or(default_method);
    let Ok(target) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) else {
        return EntryState::Missing;
    };

    let source = if entry.encrypted {
        repo_root.join(format!("{}.enc", entry.source))
    } else {
        repo_root.join(&entry.source)
    };

    if !target.exists() && !crate::fs::is_symlink(&target) {
        return EntryState::Missing;
    }

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
            // Copy mode (or encrypted, which is always copy)
            if crate::fs::is_symlink(&target) {
                EntryState::Conflict
            } else if entry.encrypted {
                // For encrypted files, we can't easily check if content matches
                // without decrypting. Just check existence.
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
    let source = repo_root.join(format!("{}.enc", entry.source));

    if !source.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!("encrypted source `{}` not found", source.display()),
        });
    }

    let encrypted = fs::read(&source).map_err(|e| Error::io(&source, "read encrypted file", e))?;
    let master_key = crate::crypto::vault::unlock_vault(password)?;
    let plaintext = crate::crypto::decrypt_with_key(&encrypted, &master_key)?;

    crate::fs::atomic_write(&target, &plaintext)?;

    // Set restrictive permissions on decrypted files (Unix)
    if let Some(perms) = entry.permissions {
        crate::fs::set_permissions(&target, perms)?;
    } else {
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

/// Undeploy an entry (remove the deployed file/symlink).
pub fn undeploy_entry(entry: &Entry) -> Result<()> {
    let target = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;

    if !target.exists() && !crate::fs::is_symlink(&target) {
        return Ok(()); // Nothing to undeploy
    }

    if crate::fs::is_symlink(&target) {
        crate::fs::remove_symlink(&target)?;
    } else if target.is_dir() {
        fs::remove_dir_all(&target).map_err(|e| Error::io(&target, "remove directory", e))?;
    } else {
        fs::remove_file(&target).map_err(|e| Error::io(&target, "remove file", e))?;
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
