use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, DeployMethod, Entry},
    error::{Error, Result},
    path, store, ui,
};

/// Add files or directories to dotling tracking.
pub fn run(paths: &[PathBuf], encrypt: bool, copy: bool, os: Option<&str>) -> Result<()> {
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

        let mut final_perms = None;
        if let Ok(Some(perms)) = crate::fs::get_permissions(&resolved) {
            final_perms = Some(perms);
        }

        if resolved.is_dir() {
            match add_directory(
                &resolved,
                &repo_root,
                &mut config,
                encrypt,
                copy,
                os,
                final_perms,
            ) {
                Ok(n) => added += n,
                Err(e) => {
                    ui::error(&format!("{e}"));
                    errors += 1;
                }
            }
        } else {
            match add_file(
                &resolved,
                &repo_root,
                &mut config,
                encrypt,
                copy,
                os,
                final_perms,
            ) {
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
#[allow(clippy::too_many_arguments)]
fn add_file(
    file_path: &Path,
    repo_root: &Path,
    config: &mut Config,
    encrypt: bool,
    copy: bool,
    os: Option<&str>,
    permissions: Option<u32>,
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
    let master_key = if encrypt {
        let password = ui::password("Vault password");
        let mk = crate::crypto::vault::unlock_vault(&password)?;
        let content = fs::read(file_path).map_err(|e| Error::io(file_path, "read file", e))?;
        let encrypted = crate::crypto::encrypt_with_key(&content, &mk)?;

        if let Some(parent) = repo_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
        }
        crate::fs::atomic_write(&repo_dest, &encrypted)?;
        Some(mk)
    } else {
        if let Some(parent) = repo_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
        }
        fs::copy(file_path, &repo_dest).map_err(|e| Error::io(file_path, "copy to repo", e))?;
        None
    };

    // Add to config
    let method = if copy { Some(DeployMethod::Copy) } else { None };

    let entry = Entry {
        source: source_str.clone(),
        target: target_str.clone(),
        method,
        encrypted: encrypt,
        directory: false,
        os: os.map(String::from),
        permissions,
    };

    config.add_entry(entry)?;

    // Deploy: remove original and create symlink/copy
    let expanded_target = path::expand_tilde(Path::new(&target_str))?;
    fs::remove_file(&expanded_target)
        .map_err(|e| Error::io(&expanded_target, "remove original", e))?;

    if encrypt || copy {
        if encrypt {
            // For encrypted files, decrypt and write using the same master key
            let mk = master_key.unwrap();
            let encrypted =
                fs::read(&repo_dest).map_err(|e| Error::io(&repo_dest, "read encrypted", e))?;
            let plaintext = crate::crypto::decrypt_with_key(&encrypted, &mk)?;
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

    if let Some(perms) = permissions {
        crate::fs::set_permissions(&expanded_target, perms)?;
    }

    ui::success(&format!("{source_str} → {target_str}"));

    Ok(())
}

/// Add a directory to tracking.
#[allow(clippy::too_many_arguments)]
fn add_directory(
    dir_path: &Path,
    repo_root: &Path,
    config: &mut Config,
    encrypt: bool,
    copy: bool,
    os: Option<&str>,
    permissions: Option<u32>,
) -> Result<u32> {
    let repo_relative = path::map_to_repo(dir_path)?;
    let target = path::collapse_tilde(dir_path);
    let target_str = target.to_string_lossy().to_string();
    let source_str = repo_relative.to_string_lossy().to_string();

    // Use directory as a single symlink unit
    let repo_dest = repo_root.join(&source_str);

    // Copy directory to repo
    if let Some(parent) = repo_dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
    }

    copy_dir_recursive(dir_path, &repo_dest)?;

    // If --encrypt was requested, encrypt every file in the repo copy
    if encrypt {
        let password = ui::password("Vault password");
        let master_key = crate::crypto::vault::unlock_vault(&password)?;
        encrypt_dir_recursive(&repo_dest, &master_key)?;
    }

    // Add single entry
    let method = if copy { Some(DeployMethod::Copy) } else { None };

    let entry = Entry {
        source: source_str.clone(),
        target: target_str.clone(),
        method,
        encrypted: encrypt,
        directory: true,
        os: os.map(String::from),
        permissions,
    };

    config.add_entry(entry)?;

    // Deploy: remove original dir and create symlink/copy
    let expanded_target = path::expand_tilde(Path::new(&target_str))?;
    fs::remove_dir_all(&expanded_target)
        .map_err(|e| Error::io(&expanded_target, "remove original directory", e))?;

    if encrypt || copy {
        if encrypt {
            // For encrypted directories, decrypt the repo copy back to the target
            let password = ui::password("Vault password (confirm for deploy)");
            let master_key = crate::crypto::vault::unlock_vault(&password)?;
            copy_dir_and_decrypt(&repo_dest, &expanded_target, &master_key)?;
        } else {
            copy_dir_recursive(&repo_dest, &expanded_target)?;
        }
    } else {
        crate::fs::create_symlink(&repo_dest, &expanded_target)?;
    }

    if let Some(perms) = permissions {
        crate::fs::set_permissions(&expanded_target, perms)?;
    }

    let label = if encrypt {
        " (directory, encrypted)"
    } else {
        " (directory)"
    };
    ui::success(&format!("{source_str} → {target_str}{label}"));

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

/// Recursively encrypt all files in `dir` in-place (each file → file.enc).
fn encrypt_dir_recursive(dir: &Path, key: &[u8; 32]) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();

        if path.is_dir() {
            encrypt_dir_recursive(&path, key)?;
        } else {
            if path.extension().and_then(|e| e.to_str()) == Some("enc") {
                continue;
            }

            let content = fs::read(&path).map_err(|e| Error::io(&path, "read", e))?;
            let encrypted = crate::crypto::encrypt_with_key(&content, key)?;

            let enc_path = match path.extension().and_then(|e| e.to_str()) {
                Some(ext) => path.with_extension(format!("{ext}.enc")),
                None => path.with_extension("enc"),
            };
            crate::fs::atomic_write(&enc_path, &encrypted)?;
            fs::remove_file(&path).map_err(|e| Error::io(&path, "remove plaintext", e))?;
        }
    }
    Ok(())
}

/// Recursively copy `src` → `dst`, decrypting `.enc` files back to plaintext.
fn copy_dir_and_decrypt(src: &Path, dst: &Path, key: &[u8; 32]) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;

    for entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(src, "read directory entry", e))?;
        let src_path = entry.path();
        let file_name = entry.file_name();

        if src_path.is_dir() {
            let dst_path = dst.join(&file_name);
            copy_dir_and_decrypt(&src_path, &dst_path, key)?;
        } else if src_path.extension().and_then(|e| e.to_str()) == Some("enc") {
            // Decrypt and write without the .enc extension
            let encrypted =
                fs::read(&src_path).map_err(|e| Error::io(&src_path, "read encrypted", e))?;
            let plaintext = crate::crypto::decrypt_with_key(&encrypted, key)?;

            // Strip .enc from the destination filename
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
            // Plain file, just copy
            let dst_path = dst.join(&file_name);
            crate::fs::copy_file(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
