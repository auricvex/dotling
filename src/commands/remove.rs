use std::{fs, path::Path};

use crate::{
    config::Config,
    error::{Error, Result},
    store, ui,
};

/// Remove entries from tracking.
///
/// Unlinks the deployed target, restores the file to its original location,
/// removes the file from the repo, and removes the entry from config.
#[allow(clippy::too_many_lines)]
pub fn run(entries: &[String]) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let mut config = Config::load(&config_path)?;

    let mut removed = 0usize;
    let mut errors = 0usize;
    let mut password_cache: Option<String> = None;

    for query in entries {
        // Find by source or target
        let entry = config.find_entry(query).cloned();

        if let Some(entry) = entry {
            let target = crate::path::expand_tilde(Path::new(&entry.target))?;
            let target_is_symlink = crate::fs::is_symlink(&target);
            let target_exists = target.exists();

            let repo_source = if entry.encrypted && !entry.directory {
                repo_root.join(format!("{}.enc", entry.source))
            } else {
                repo_root.join(&entry.source)
            };

            // We need to restore the target if it does not exist or if it is currently a symlink.
            // If it is already a regular file/directory, we do not touch it (to avoid overwriting
            // local changes).
            let mut restore_success = true;

            if !target_exists || target_is_symlink {
                if target_is_symlink {
                    if let Err(e) = crate::fs::remove_symlink(&target) {
                        ui::error(&format!(
                            "could not remove symlink `{}`: {e}",
                            target.display()
                        ));
                        errors += 1;
                        continue;
                    }
                }

                if repo_source.exists() {
                    let res = if entry.encrypted {
                        let password = if let Some(p) = &password_cache {
                            p.clone()
                        } else {
                            let p = ui::password(&format!(
                                "Vault password to decrypt restored entry `{}`",
                                entry.source
                            ));
                            password_cache = Some(p.clone());
                            p
                        };
                        match crate::crypto::vault::unlock_vault(&password) {
                            Ok(master_key) => {
                                if entry.directory {
                                    decrypt_dir_to_target(&repo_source, &target, &master_key)
                                } else {
                                    match fs::read(&repo_source) {
                                        Ok(encrypted) => {
                                            match crate::crypto::decrypt_with_key(
                                                &encrypted,
                                                &master_key,
                                            ) {
                                                Ok(plaintext) => {
                                                    let write_res = crate::fs::atomic_write(
                                                        &target, &plaintext,
                                                    );
                                                    if write_res.is_ok() {
                                                        if let Some(perms) = entry.permissions {
                                                            crate::fs::set_permissions(
                                                                &target, perms,
                                                            )
                                                            .ok();
                                                        } else {
                                                            #[cfg(unix)]
                                                            {
                                                                use std::os::unix::fs::PermissionsExt;
                                                                let perms =
                                                                    std::fs::Permissions::from_mode(
                                                                        0o600,
                                                                    );
                                                                std::fs::set_permissions(
                                                                    &target, perms,
                                                                )
                                                                .ok();
                                                            }
                                                        }
                                                    }
                                                    write_res
                                                }
                                                Err(e) => Err(e),
                                            }
                                        }
                                        Err(e) => Err(Error::io(&repo_source, "read encrypted", e)),
                                    }
                                }
                            }
                            Err(e) => Err(e),
                        }
                    } else if entry.directory {
                        copy_dir_recursive(&repo_source, &target)
                    } else {
                        let copy_res = crate::fs::copy_file(&repo_source, &target);
                        if copy_res.is_ok() {
                            if let Some(perms) = entry.permissions {
                                crate::fs::set_permissions(&target, perms).ok();
                            }
                        }
                        copy_res
                    };

                    if let Err(e) = res {
                        ui::error(&format!(
                            "failed to restore `{}` to `{}`: {e}",
                            entry.source,
                            target.display()
                        ));
                        restore_success = false;
                    }
                } else {
                    ui::warning(&format!(
                        "source `{}` does not exist in repo; could not restore, but will remove from tracking",
                        entry.source
                    ));
                }
            }

            if !restore_success {
                errors += 1;
                continue;
            }

            // Remove the file from the repo now that it has been restored successfully
            if repo_source.exists() {
                if repo_source.is_dir() {
                    if let Err(e) = fs::remove_dir_all(&repo_source) {
                        ui::warning(&format!(
                            "could not remove `{}` from repo: {e}",
                            repo_source.display()
                        ));
                    }
                } else if let Err(e) = fs::remove_file(&repo_source) {
                    ui::warning(&format!(
                        "could not remove `{}` from repo: {e}",
                        repo_source.display()
                    ));
                }
            }

            // Clean up empty parent dirs left in repo
            crate::fs::cleanup_empty_parents(&repo_source, &repo_root).ok();

            config.remove_entry(&entry.source);
            ui::success(&format!("removed `{}`", entry.source));
            removed += 1;
        } else {
            ui::error(&format!("`{query}` is not tracked"));
            errors += 1;
        }
    }

    config.save()?;
    ui::summary(removed, 0, errors);

    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| crate::error::Error::io(dst, "create directory", e))?;

    for entry in
        std::fs::read_dir(src).map_err(|e| crate::error::Error::io(src, "read directory", e))?
    {
        let entry = entry.map_err(|e| crate::error::Error::io(src, "read entry", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            crate::fs::copy_file(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Recursively decrypt all `.enc` files in a directory to target.
fn decrypt_dir_to_target(src: &Path, dst: &Path, key: &[u8; 32]) -> Result<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| crate::error::Error::io(dst, "create directory", e))?;

    for entry in
        std::fs::read_dir(src).map_err(|e| crate::error::Error::io(src, "read directory", e))?
    {
        let entry = entry.map_err(|e| crate::error::Error::io(src, "read entry", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            decrypt_dir_to_target(&src_path, &dst_path, key)?;
        } else if src_path.extension().and_then(|e| e.to_str()) == Some("enc") {
            let encrypted = std::fs::read(&src_path)
                .map_err(|e| crate::error::Error::io(&src_path, "read encrypted", e))?;
            let plaintext = crate::crypto::decrypt_with_key(&encrypted, key)?;
            let dst_path_original = dst_path.with_extension("");
            crate::fs::atomic_write(&dst_path_original, &plaintext)?;
            // Set 0o600 for decrypted files on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(&dst_path_original, perms).map_err(|e| {
                    crate::error::Error::io(&dst_path_original, "set permissions", e)
                })?;
            }
        } else {
            crate::fs::copy_file(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::disallowed_types)]
mod tests {
    use std::{fs, sync::Mutex};

    use tempfile::tempdir;

    use super::*;
    use crate::config::{Config, DeployMethod, Entry};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_remove_symlink_file() {
        let _guard = TEST_LOCK.lock().unwrap();

        let home_temp = tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", home_temp.path());
        }

        let repo_temp = tempdir().unwrap();
        crate::store::set_repo_root(repo_temp.path()).unwrap();

        let config_path = crate::store::config_path(repo_temp.path());
        let mut config = Config::new(config_path.clone());

        let entry = Entry {
            source: "shell/zshrc".into(),
            target: home_temp.path().join(".zshrc").to_str().unwrap().into(),
            method: None,
            encrypted: false,
            directory: false,
            os: None,
            permissions: None,
        };
        config.add_entry(entry.clone()).unwrap();
        config.save().unwrap();

        // Create repository file
        let repo_source = repo_temp.path().join(&entry.source);
        fs::create_dir_all(repo_source.parent().unwrap()).unwrap();
        fs::write(&repo_source, "zshrc repository content").unwrap();

        // Create symlink at target pointing to repository file
        let target_path = home_temp.path().join(".zshrc");
        crate::fs::create_symlink(&repo_source, &target_path).unwrap();

        // Run remove command
        run(&["shell/zshrc".to_string()]).unwrap();

        // Target should be restored as a regular file with the content from repo
        assert!(!crate::fs::is_symlink(&target_path));
        assert!(target_path.exists());
        assert_eq!(
            fs::read_to_string(&target_path).unwrap(),
            "zshrc repository content"
        );

        // Repo source should be deleted
        assert!(!repo_source.exists());

        // Entry should be removed from config
        let updated_config = Config::load(&config_path).unwrap();
        assert!(updated_config.entries.is_empty());
    }

    #[test]
    fn test_remove_copy_file() {
        let _guard = TEST_LOCK.lock().unwrap();

        let home_temp = tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", home_temp.path());
        }

        let repo_temp = tempdir().unwrap();
        crate::store::set_repo_root(repo_temp.path()).unwrap();

        let config_path = crate::store::config_path(repo_temp.path());
        let mut config = Config::new(config_path.clone());

        let entry = Entry {
            source: "config/nvim/init.lua".into(),
            target: home_temp
                .path()
                .join(".config/nvim/init.lua")
                .to_str()
                .unwrap()
                .into(),
            method: Some(DeployMethod::Copy),
            encrypted: false,
            directory: false,
            os: None,
            permissions: None,
        };
        config.add_entry(entry.clone()).unwrap();
        config.save().unwrap();

        // Create repository file
        let repo_source = repo_temp.path().join(&entry.source);
        fs::create_dir_all(repo_source.parent().unwrap()).unwrap();
        fs::write(&repo_source, "nvim repo content").unwrap();

        // Create regular file at target with modified content (local edits)
        let target_path = home_temp.path().join(".config/nvim/init.lua");
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        fs::write(&target_path, "nvim local edited content").unwrap();

        // Run remove command
        run(&["config/nvim/init.lua".to_string()]).unwrap();

        // Target should be preserved with local edits, not overwritten by repo content!
        assert!(!crate::fs::is_symlink(&target_path));
        assert!(target_path.exists());
        assert_eq!(
            fs::read_to_string(&target_path).unwrap(),
            "nvim local edited content"
        );

        // Repo source should be deleted
        assert!(!repo_source.exists());

        // Entry should be removed from config
        let updated_config = Config::load(&config_path).unwrap();
        assert!(updated_config.entries.is_empty());
    }

    #[test]
    fn test_remove_symlink_directory() {
        let _guard = TEST_LOCK.lock().unwrap();

        let home_temp = tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", home_temp.path());
        }

        let repo_temp = tempdir().unwrap();
        crate::store::set_repo_root(repo_temp.path()).unwrap();

        let config_path = crate::store::config_path(repo_temp.path());
        let mut config = Config::new(config_path.clone());

        let entry = Entry {
            source: "config/nvim".into(),
            target: home_temp
                .path()
                .join(".config/nvim")
                .to_str()
                .unwrap()
                .into(),
            method: None,
            encrypted: false,
            directory: true,
            os: None,
            permissions: None,
        };
        config.add_entry(entry.clone()).unwrap();
        config.save().unwrap();

        // Create repository directory and files inside
        let repo_source = repo_temp.path().join(&entry.source);
        fs::create_dir_all(repo_source.join("lua")).unwrap();
        fs::write(repo_source.join("init.lua"), "init content").unwrap();
        fs::write(repo_source.join("lua/utils.lua"), "utils content").unwrap();

        // Create symlink at target pointing to repository directory
        let target_path = home_temp.path().join(".config/nvim");
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        crate::fs::create_symlink(&repo_source, &target_path).unwrap();

        // Run remove command
        run(&["config/nvim".to_string()]).unwrap();

        // Target directory should be restored as a regular directory recursively
        assert!(!crate::fs::is_symlink(&target_path));
        assert!(target_path.is_dir());
        assert_eq!(
            fs::read_to_string(target_path.join("init.lua")).unwrap(),
            "init content"
        );
        assert_eq!(
            fs::read_to_string(target_path.join("lua/utils.lua")).unwrap(),
            "utils content"
        );

        // Repo source should be deleted
        assert!(!repo_source.exists());

        // Entry should be removed from config
        let updated_config = Config::load(&config_path).unwrap();
        assert!(updated_config.entries.is_empty());
    }
}
