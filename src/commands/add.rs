use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{Config, DeployMethod, Entry},
    error::{Error, Result},
    path, store, ui,
    vars::VarStore,
};

/// Add files or directories to dotling tracking.
#[allow(clippy::fn_params_excessive_bools)]
pub fn run(
    paths: &[PathBuf],
    encrypt: bool,
    copy: bool,
    template: bool,
    os: Option<&str>,
) -> Result<()> {
    // Directories cannot be templates.
    let has_dir = paths
        .iter()
        .any(|p| path::resolve(p).is_ok_and(|r| r.is_dir()));
    if template && has_dir {
        return Err(Error::User(
            "--template is not supported for directories".into(),
        ));
    }

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
        } else if template {
            match add_template_file(&resolved, &repo_root, &mut config, encrypt, os, final_perms) {
                Ok(()) => added += 1,
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

// ── Template file ──────────────────────────────────────────────────

/// Add a file as a template entry.
///
/// Pipeline:
/// 1. Validate template syntax and warn if no `{{ }}` tags found.
/// 2. Copy source into repo at the mapped path.
/// 3. Remove the original file.
/// 4. Render the template immediately and write rendered output to target.
#[allow(clippy::too_many_arguments)]
fn add_template_file(
    file_path: &Path,
    repo_root: &Path,
    config: &mut Config,
    encrypt: bool,
    os: Option<&str>,
    permissions: Option<u32>,
) -> Result<()> {
    // Read the template source
    let template_text =
        fs::read_to_string(file_path).map_err(|e| Error::io(file_path, "read template file", e))?;

    // Scan for variables — warn if none found
    let vars_found = crate::template::scan_variables(&template_text);
    if vars_found.is_empty() {
        ui::warning(&format!(
            "`{}` has no template variables (`{{{{ }}}}`) — add with --template anyway",
            file_path.display()
        ));
    }

    let repo_relative = path::map_to_repo(file_path)?;
    let target = path::collapse_tilde(file_path);
    let target_str = target.to_string_lossy().to_string();

    // Source in repo: use the mapped path as-is
    let source_str = repo_relative.to_string_lossy().to_string();

    let repo_dest = repo_root.join(&source_str);

    // Ensure parent dir exists
    if let Some(parent) = repo_dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
    }

    // Store the template source in the repo (encrypt if requested)
    let master_key = if encrypt {
        let password = ui::password("Vault password");
        let mk = crate::crypto::vault::unlock_vault(&password)?;
        let content = template_text.as_bytes().to_vec();
        let encrypted = crate::crypto::encrypt_with_key(&content, &mk)?;
        crate::fs::atomic_write(&repo_dest, &encrypted)?;
        Some(mk)
    } else {
        crate::fs::atomic_write(&repo_dest, template_text.as_bytes())?;
        None
    };

    // Add to config (template = true because --template flag was passed)
    let entry = Entry {
        source: source_str.clone(),
        target: target_str.clone(),
        method: Some(DeployMethod::Copy), // templates are always copy-deployed
        encrypted: encrypt,
        directory: false,
        template: true,
        os: os.map(String::from),
        permissions,
        before: None,
        after: None,
    };

    config.add_entry(entry)?;

    // Render the template immediately and deploy
    let expanded_target = path::expand_tilde(Path::new(&target_str))?;

    // Remove the original file
    fs::remove_file(file_path).map_err(|e| Error::io(file_path, "remove original", e))?;

    let rendered = if encrypt {
        // Decrypt for rendering
        let mk = master_key.unwrap();
        let encrypted = fs::read(&repo_dest)
            .map_err(|e| Error::io(&repo_dest, "read encrypted template", e))?;
        let plaintext_bytes = crate::crypto::decrypt_with_key(&encrypted, &mk)?;
        let plaintext = String::from_utf8(plaintext_bytes).map_err(|_| Error::Template {
            source: source_str.clone(),
            message: "template is not valid UTF-8".into(),
        })?;
        render_with_context(&plaintext, &source_str, config)?
    } else {
        render_with_context(&template_text, &source_str, config)?
    };

    crate::fs::atomic_write(&expanded_target, rendered.as_bytes())?;

    if let Some(perms) = permissions {
        crate::fs::set_permissions(&expanded_target, perms)?;
    }

    let label = if encrypt {
        " (template, encrypted)"
    } else {
        " (template)"
    };
    ui::success(&format!("{source_str} → {target_str}{label}"));

    Ok(())
}

/// Build a render context and render template text, reporting unresolved
/// variables with fix hints.
fn render_with_context(template_text: &str, source_name: &str, config: &Config) -> Result<String> {
    // Load local var store (best-effort; missing file is fine)
    let local_vars = VarStore::load().map(|s| s.as_pairs()).unwrap_or_default();

    // Determine repo root string for the context
    let repo_root_str = store::get_repo_root()
        .ok()
        .flatten()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let ctx = crate::template::RenderContext::new(&repo_root_str, &config.vars, &local_vars);

    // Scan variables first so we can give a nice error with fix hints
    let vars = crate::template::scan_variables(template_text);
    let mut missing: Vec<String> = Vec::new();
    for var in &vars {
        if var.namespace == "var" && ctx.resolve("var", &var.key).is_none() {
            missing.push(var.key.clone());
        }
        if var.namespace == "dotling" && !ctx.builtins.contains_key(&var.key) {
            return Err(Error::Template {
                source: source_name.to_string(),
                message: format!("unknown built-in `dotling.{}`", var.key),
            });
        }
    }

    if !missing.is_empty() {
        // Print hints before the hard error
        for key in &missing {
            ui::hint(&format!("  → dotling vars set {key} <value>"));
        }
        return Err(Error::Template {
            source: source_name.to_string(),
            message: format!(
                "unresolved variable{}: {} — run `dotling vars set <key> <value>` then sync",
                if missing.len() == 1 { "" } else { "s" },
                missing
                    .iter()
                    .map(|k| format!("var.{k}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    crate::template::render(template_text, &ctx, source_name)
}

// ── Plain file ─────────────────────────────────────────────────────

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
    let repo_dest = repo_root.join(&source_str);

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
        template: false,
        os: os.map(String::from),
        permissions,
        before: None,
        after: None,
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

// ── Directory ─────────────────────────────────────────────────────

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
        template: false,
        os: os.map(String::from),
        permissions,
        before: None,
        after: None,
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

/// Recursively encrypt all files in `dir` in-place.
fn encrypt_dir_recursive(dir: &Path, key: &[u8; 32]) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();

        if path.is_dir() {
            encrypt_dir_recursive(&path, key)?;
        } else {
            let content = fs::read(&path).map_err(|e| Error::io(&path, "read", e))?;
            if crate::crypto::is_encrypted_content(&content) {
                continue;
            }
            let encrypted = crate::crypto::encrypt_with_key(&content, key)?;
            crate::fs::atomic_write(&path, &encrypted)?;
        }
    }
    Ok(())
}

/// Recursively copy `src` → `dst`, decrypting encrypted files back to plaintext.
fn copy_dir_and_decrypt(src: &Path, dst: &Path, key: &[u8; 32]) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;

    for entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(src, "read directory entry", e))?;
        let src_path = entry.path();
        let file_name = entry.file_name();

        if src_path.is_dir() {
            let dst_path = dst.join(&file_name);
            copy_dir_and_decrypt(&src_path, &dst_path, key)?;
        } else {
            let content = fs::read(&src_path).map_err(|e| Error::io(&src_path, "read", e))?;
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
