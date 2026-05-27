use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{Config, DeployMethod, Entry};
use crate::error::{Error, Result};
use crate::{path, store, ui};

/// Add files or directories to dotling tracking.
pub fn run(
    paths: &[PathBuf],
    encrypt: bool,
    copy: bool,
    os: Option<&str>,
) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let mut config = Config::load(&config_path)?;

    let mut added = 0u32;
    let mut errors = 0u32;

    for input_path in paths {
        let resolved = path::resolve(input_path)?;

        if !resolved.exists() {
            ui::error(&format!("`{}` does not exist", input_path.display()));
            errors += 1;
            continue;
        }

        if resolved.is_dir() {
            match add_directory(
                &resolved,
                &repo_root,
                &mut config,
                encrypt,
                copy,
                os,
            ) {
                Ok(n) => added += n,
                Err(e) => {
                    ui::error(&format!("{e}"));
                    errors += 1;
                }
            }
        } else {
            match add_file(&resolved, &repo_root, &mut config, encrypt, copy, os) {
                Ok(()) => added += 1,
                Err(e) => {
                    ui::error(&format!("{e}"));
                    errors += 1;
                }
            }
        }
    }

    config.save()?;
    ui::summary(added as usize, 0, errors as usize);

    Ok(())
}

/// Add a single file to tracking.
fn add_file(
    file_path: &Path,
    repo_root: &Path,
    config: &mut Config,
    encrypt: bool,
    copy: bool,
    os: Option<&str>,
) -> Result<()> {
    let repo_relative = path::map_to_repo(file_path)?;
    let target = path::collapse_tilde(file_path);
    let target_str = target.to_string_lossy().to_string();
    let source_str = repo_relative.to_string_lossy().to_string();

    // Check if the source already exists in the repo
    let repo_dest = if encrypt {
        repo_root.join(format!("{source_str}.enc"))
    } else {
        repo_root.join(&source_str)
    };

    // Move the file into the repo
    if encrypt {
        // Encrypt and store
        let password = ui::password("Vault password");
        let content =
            fs::read(file_path).map_err(|e| Error::io(file_path, "read file", e))?;
        let encrypted = crate::crypto::encrypt(&content, &password)?;

        if let Some(parent) = repo_dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::io(parent, "create directory", e))?;
        }
        crate::fs::atomic_write(&repo_dest, &encrypted)?;
    } else {
        if let Some(parent) = repo_dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::io(parent, "create directory", e))?;
        }
        fs::copy(file_path, &repo_dest)
            .map_err(|e| Error::io(file_path, "copy to repo", e))?;
    }

    // Add to config
    let method = if copy {
        Some(DeployMethod::Copy)
    } else {
        None
    };

    let entry = Entry {
        source: source_str.clone(),
        target: target_str.clone(),
        method,
        encrypted: encrypt,
        directory: false,
        os: os.map(String::from),
    };

    config.add_entry(entry)?;

    // Deploy: remove original and create symlink/copy
    let expanded_target = path::expand_tilde(Path::new(&target_str))?;
    fs::remove_file(&expanded_target)
        .map_err(|e| Error::io(&expanded_target, "remove original", e))?;

    if encrypt || copy {
        if encrypt {
            // For encrypted files, decrypt and write
            let encrypted = fs::read(&repo_dest)
                .map_err(|e| Error::io(&repo_dest, "read encrypted", e))?;
            let password = ui::password("Vault password (confirm)");
            let plaintext = crate::crypto::decrypt(&encrypted, &password)?;
            crate::fs::atomic_write(&expanded_target, &plaintext)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                fs::set_permissions(&expanded_target, perms).ok();
            }
        } else {
            crate::fs::copy_file(&repo_dest, &expanded_target)?;
        }
    } else {
        crate::fs::create_symlink(&repo_dest, &expanded_target)?;
    }

    ui::success(&format!("{source_str} → {target_str}"));

    Ok(())
}

/// Add a directory to tracking.
fn add_directory(
    dir_path: &Path,
    repo_root: &Path,
    config: &mut Config,
    encrypt: bool,
    copy: bool,
    os: Option<&str>,
) -> Result<u32> {
    if encrypt {
        return Err(Error::User(
            "cannot encrypt entire directories — add individual files with --encrypt".into(),
        ));
    }

    let repo_relative = path::map_to_repo(dir_path)?;
    let target = path::collapse_tilde(dir_path);
    let target_str = target.to_string_lossy().to_string();
    let source_str = repo_relative.to_string_lossy().to_string();

    // Use directory as a single symlink unit
    let repo_dest = repo_root.join(&source_str);

    // Copy directory to repo
    if let Some(parent) = repo_dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::io(parent, "create directory", e))?;
    }

    copy_dir_recursive(dir_path, &repo_dest)?;

    // Add single entry
    let method = if copy {
        Some(DeployMethod::Copy)
    } else {
        None
    };

    let entry = Entry {
        source: source_str.clone(),
        target: target_str.clone(),
        method,
        encrypted: false,
        directory: true,
        os: os.map(String::from),
    };

    config.add_entry(entry)?;

    // Deploy: remove original dir and create symlink
    let expanded_target = path::expand_tilde(Path::new(&target_str))?;
    fs::remove_dir_all(&expanded_target)
        .map_err(|e| Error::io(&expanded_target, "remove original directory", e))?;

    if copy {
        copy_dir_recursive(&repo_dest, &expanded_target)?;
    } else {
        crate::fs::create_symlink(&repo_dest, &expanded_target)?;
    }

    ui::success(&format!("{source_str} → {target_str} (directory)"));

    Ok(1)
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;

    for entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(src, "read entry", e))?;
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
