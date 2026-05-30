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

    // Source is always the path stored in entry.source — no extensions appended.
    let source = repo_root.join(&entry.source);

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
            } else if entry.template {
                // For templates, the deployed file is rendered output.
                // Fingerprint-based comparison happens in status/sync.
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
        let source = repo_root.join(&entry.source);
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
        } else {
            let content = fs::read(&src_path).map_err(|e| Error::io(&src_path, "read file", e))?;
            let dst_path = dst.join(&file_name);

            if crate::crypto::is_encrypted_content(&content) {
                let plaintext = crate::crypto::decrypt_with_key(&content, key)?;
                crate::fs::atomic_write(&dst_path, &plaintext)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(0o600);
                    fs::set_permissions(&dst_path, perms).ok();
                }
            } else {
                crate::fs::copy_file(&src_path, &dst_path)?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        source: &str,
        target: &str,
        method: Option<DeployMethod>,
        encrypted: bool,
    ) -> Entry {
        Entry {
            source: source.into(),
            target: target.into(),
            method,
            encrypted,
            directory: false,
            template: false,
            os: None,
            permissions: None,
            before: None,
            after: None,
        }
    }

    fn make_template_entry(source: &str, target: &str, encrypted: bool) -> Entry {
        Entry {
            source: source.into(),
            target: target.into(),
            method: None,
            encrypted,
            directory: false,
            template: true,
            os: None,
            permissions: None,
            before: None,
            after: None,
        }
    }

    // ── check_state tests ───────────────────────────────────────

    #[test]
    fn state_symlink_deployed() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "config").unwrap();
        crate::fs::create_symlink(&source, &target).unwrap();

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        let state = check_state(&entry, &repo, DeployMethod::Symlink);
        assert_eq!(state, EntryState::Deployed);
    }

    #[test]
    fn state_symlink_broken() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let target = temp.path().join(".zshrc");
        let bad_source = repo.join("nonexistent");
        crate::fs::create_symlink(&bad_source, &target).unwrap();

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        let state = check_state(&entry, &repo, DeployMethod::Symlink);
        assert_eq!(state, EntryState::Broken);
    }

    #[test]
    fn state_symlink_wrong_target() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let wrong = repo.join("other");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "config").unwrap();
        crate::fs::create_symlink(&wrong, &target).unwrap();

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        let state = check_state(&entry, &repo, DeployMethod::Symlink);
        assert_eq!(state, EntryState::Broken);
    }

    #[test]
    fn state_missing() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let target = temp.path().join(".zshrc");

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        let state = check_state(&entry, &repo, DeployMethod::Copy);
        assert_eq!(state, EntryState::Missing);
    }

    #[test]
    fn state_copy_deployed() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "same content").unwrap();
        fs::write(&target, "same content").unwrap();

        let entry = make_entry(
            "zshrc",
            &target.to_string_lossy(),
            Some(DeployMethod::Copy),
            false,
        );
        let state = check_state(&entry, &repo, DeployMethod::Copy);
        assert_eq!(state, EntryState::Deployed);
    }

    #[test]
    fn state_copy_modified() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "repo version").unwrap();
        fs::write(&target, "user modified").unwrap();

        let entry = make_entry(
            "zshrc",
            &target.to_string_lossy(),
            Some(DeployMethod::Copy),
            false,
        );
        let state = check_state(&entry, &repo, DeployMethod::Copy);
        assert_eq!(state, EntryState::Modified);
    }

    #[test]
    fn state_conflict_regular_at_symlink_path() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "config").unwrap();
        fs::write(&target, "user file").unwrap(); // regular file, not a symlink

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        let state = check_state(&entry, &repo, DeployMethod::Symlink);
        assert_eq!(state, EntryState::Conflict);
    }

    #[test]
    fn state_template_entry() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "template").unwrap();
        fs::write(&target, "rendered").unwrap();

        let entry = make_template_entry("zshrc", &target.to_string_lossy(), false);
        let state = check_state(&entry, &repo, DeployMethod::Symlink);
        assert_eq!(state, EntryState::Deployed);
    }

    // ── deploy_entry tests ──────────────────────────────────────

    #[test]
    fn deploy_symlink_creates_link() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "config").unwrap();

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        deploy_entry(&entry, &repo, DeployMethod::Symlink, false).unwrap();

        assert!(crate::fs::is_symlink(&target));
        assert_eq!(crate::fs::read_link(&target).unwrap(), source);
    }

    #[test]
    fn deploy_copy_copies_file() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "config data").unwrap();

        let entry = make_entry(
            "zshrc",
            &target.to_string_lossy(),
            Some(DeployMethod::Copy),
            false,
        );
        deploy_entry(&entry, &repo, DeployMethod::Copy, false).unwrap();

        assert!(!crate::fs::is_symlink(&target));
        assert_eq!(fs::read_to_string(&target).unwrap(), "config data");
    }

    #[test]
    fn deploy_creates_parent_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("config");
        let target = temp.path().join("deep/nested/.config");
        fs::write(&source, "data").unwrap();

        let entry = make_entry(
            "config",
            &target.to_string_lossy(),
            Some(DeployMethod::Copy),
            false,
        );
        deploy_entry(&entry, &repo, DeployMethod::Copy, false).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "data");
    }

    #[cfg(unix)]
    #[test]
    fn deploy_sets_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("secret");
        let target = temp.path().join(".secret");
        fs::write(&source, "data").unwrap();

        let mut entry = make_entry(
            "secret",
            &target.to_string_lossy(),
            Some(DeployMethod::Copy),
            false,
        );
        entry.permissions = Some(0o600);
        deploy_entry(&entry, &repo, DeployMethod::Copy, false).unwrap();

        let perms = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(perms, 0o600);
    }

    #[test]
    fn deploy_force_overwrites_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "new config").unwrap();
        // Regular file where a symlink should be → Conflict
        fs::write(&target, "user file").unwrap();

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);

        // Without force: Conflict → should error
        let result = deploy_entry(&entry, &repo, DeployMethod::Symlink, false);
        assert!(result.is_err());

        // With force: should succeed
        deploy_entry(&entry, &repo, DeployMethod::Symlink, true).unwrap();
        assert!(crate::fs::is_symlink(&target));
    }

    #[test]
    fn deploy_already_deployed_noop() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let source = repo.join("zshrc");
        let target = temp.path().join(".zshrc");
        fs::write(&source, "config").unwrap();
        crate::fs::create_symlink(&source, &target).unwrap();

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        // Should succeed without error (already deployed)
        deploy_entry(&entry, &repo, DeployMethod::Symlink, false).unwrap();
        assert!(crate::fs::is_symlink(&target));
    }

    #[test]
    fn deploy_missing_source_error() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let target = temp.path().join(".zshrc");

        let entry = make_entry("zshrc", &target.to_string_lossy(), None, false);
        let result = deploy_entry(&entry, &repo, DeployMethod::Symlink, false);
        assert!(result.is_err());
    }
}
