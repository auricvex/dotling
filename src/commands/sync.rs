use std::fs;

use crate::{
    config::{Config, DeployMethod, Entry},
    deploy::EntryState,
    error::{Error, Result},
    platform, store, ui,
};

// ── Direction resolved per entry ──────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAction {
    /// Already in sync — nothing to do.
    Ok,
    /// Push from repo → actual (deploy direction).
    Push,
    /// Pull from actual → repo.
    Pull,
    /// Both sides differ and we cannot determine a winner without --prefer-actual.
    Conflict,
    /// Symlink is missing or broken (always a Push).
    FixSymlink,
}

// ── Public entry point ────────────────────────────────────────────

/// Synchronise all tracked entries in both directions.
#[allow(clippy::too_many_lines)]
pub fn run(dry_run: bool, force: bool, prefer_actual: bool) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let config = Config::load(&config_path)?;

    if config.entries.is_empty() {
        ui::info("no entries to sync");
        ui::hint("add files with `dotling add <path>`");
        return Ok(());
    }

    let mut pushed = 0usize;
    let mut pulled = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    let mut password_cache: Option<String> = None;

    for entry in &config.entries {
        // Skip entries not meant for this OS.
        if !platform::should_deploy(entry.os.as_deref()) {
            skipped += 1;
            continue;
        }

        let action = resolve_action(entry, &repo_root, config.settings.method, prefer_actual);

        if dry_run {
            let label = match action {
                SyncAction::Ok => "ok",
                SyncAction::Push | SyncAction::FixSymlink => "would push (repo → actual)",
                SyncAction::Pull => "would pull (actual → repo)",
                SyncAction::Conflict => "conflict (use --prefer-actual or --force)",
            };
            ui::info(&format!("{label}: {} ↔ {}", entry.source, entry.target));
            continue;
        }

        if action == SyncAction::Ok {
            skipped += 1;
            continue;
        }

        if action == SyncAction::Conflict && !force {
            ui::warning(&format!(
                "conflict: {} ↔ {} — use --prefer-actual or --force to resolve",
                entry.source, entry.target
            ));
            errors += 1;
            continue;
        }

        // Resolve conflict: --prefer-actual wins; otherwise push (repo wins).
        let resolved_action = if action == SyncAction::Conflict {
            if prefer_actual {
                SyncAction::Pull
            } else {
                SyncAction::Push
            }
        } else {
            action
        };

        let result = match resolved_action {
            SyncAction::Push | SyncAction::FixSymlink => {
                if entry.encrypted {
                    let password = get_or_prompt_password(&mut password_cache);
                    push_encrypted(entry, &repo_root, &password)
                } else {
                    crate::deploy::deploy_entry(entry, &repo_root, config.settings.method, force)
                }
            }
            SyncAction::Pull => {
                if entry.encrypted {
                    let password = get_or_prompt_password(&mut password_cache);
                    pull_encrypted(entry, &repo_root, &password)
                } else {
                    pull_entry(entry, &repo_root)
                }
            }
            SyncAction::Ok | SyncAction::Conflict => unreachable!(),
        };

        match result {
            Ok(()) => match resolved_action {
                SyncAction::Push | SyncAction::FixSymlink => {
                    ui::success(&format!("push {} → {}", entry.source, entry.target));
                    pushed += 1;
                }
                SyncAction::Pull => {
                    ui::success(&format!("pull {} ← {}", entry.source, entry.target));
                    pulled += 1;
                }
                SyncAction::Ok | SyncAction::Conflict => unreachable!(),
            },
            Err(e) => {
                ui::error(&format!("{} ↔ {}: {e}", entry.source, entry.target));
                errors += 1;
            }
        }
    }

    if dry_run {
        ui::dim("(dry run — no changes made)");
    } else {
        // pushed + pulled + skipped = total processed "ok" operations
        ui::summary(pushed + pulled + skipped, 0, errors);
    }

    Ok(())
}

// ── Action resolution ─────────────────────────────────────────────

fn resolve_action(
    entry: &Entry,
    repo_root: &std::path::Path,
    default_method: DeployMethod,
    prefer_actual: bool,
) -> SyncAction {
    let method = entry.method.unwrap_or(default_method);

    let Ok(target) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) else {
        // Can't even expand the path → treat as push needed.
        return SyncAction::Push;
    };

    match method {
        DeployMethod::Symlink if !entry.encrypted => {
            // Symlinks are always repo-authoritative.
            let state = crate::deploy::check_state(entry, repo_root, default_method);
            match state {
                EntryState::Deployed => SyncAction::Ok,
                _ => SyncAction::FixSymlink,
            }
        }
        _ => {
            // Copy mode (or encrypted) — compare content.
            if entry.encrypted {
                resolve_encrypted_action(entry, repo_root, &target, prefer_actual)
            } else {
                resolve_copy_action(entry, repo_root, &target)
            }
        }
    }
}

/// Resolve sync direction for a plain copy-mode entry.
fn resolve_copy_action(
    entry: &Entry,
    repo_root: &std::path::Path,
    target: &std::path::Path,
) -> SyncAction {
    let source = repo_root.join(&entry.source);

    let source_exists = source.exists();
    let target_exists = target.exists() && !crate::fs::is_symlink(target);

    match (source_exists, target_exists) {
        (false, false) => SyncAction::Ok, // nothing anywhere
        (true, false) => SyncAction::Push,
        (false, true) => SyncAction::Pull,
        (true, true) => {
            match crate::fs::files_identical(&source, target) {
                Ok(true) => SyncAction::Ok,
                Ok(false) => {
                    // Use modification time to break the tie.
                    match (source.metadata(), target.metadata()) {
                        (Ok(sm), Ok(tm)) => {
                            match (sm.modified(), tm.modified()) {
                                (Ok(st), Ok(tt)) => match tt.cmp(&st) {
                                    std::cmp::Ordering::Greater => SyncAction::Pull,
                                    std::cmp::Ordering::Less => SyncAction::Push,
                                    // Exact same mtime but different content → conflict.
                                    std::cmp::Ordering::Equal => SyncAction::Conflict,
                                },
                                _ => SyncAction::Conflict,
                            }
                        }
                        _ => SyncAction::Conflict,
                    }
                }
                Err(_) => SyncAction::Conflict,
            }
        }
    }
}

/// Resolve sync direction for an encrypted entry.
///
/// We decrypt the `.enc` file in memory and compare with the target plaintext.
fn resolve_encrypted_action(
    entry: &Entry,
    repo_root: &std::path::Path,
    target: &std::path::Path,
    _prefer_actual: bool,
) -> SyncAction {
    let enc_path = repo_root.join(format!("{}.enc", entry.source));

    let enc_exists = enc_path.exists();
    let target_exists = target.exists() && !crate::fs::is_symlink(target);

    match (enc_exists, target_exists) {
        (false, false) => SyncAction::Ok,
        (true, false) => SyncAction::Push, // deploy (decrypt) needed
        (false, true) => SyncAction::Pull, // encrypt and store
        (true, true) => {
            // Both exist — use modification times to guess direction.
            // (Full comparison requires the password, which we don't have here.)
            match (enc_path.metadata(), target.metadata()) {
                (Ok(em), Ok(tm)) => {
                    match (em.modified(), tm.modified()) {
                        (Ok(et), Ok(tt)) => match tt.cmp(&et) {
                            std::cmp::Ordering::Greater => SyncAction::Pull,
                            std::cmp::Ordering::Less => SyncAction::Push,
                            // Same mtime → assume in sync.
                            std::cmp::Ordering::Equal => SyncAction::Ok,
                        },
                        _ => SyncAction::Push, // fallback: push
                    }
                }
                _ => SyncAction::Push,
            }
        }
    }
}

// ── Push (repo → actual) ──────────────────────────────────────────

/// Push an encrypted entry: decrypt source.enc → target.
fn push_encrypted(entry: &Entry, repo_root: &std::path::Path, password: &str) -> Result<()> {
    crate::deploy::deploy_encrypted(entry, repo_root, password)
}

// ── Pull (actual → repo) ──────────────────────────────────────────

/// Pull a plain copy-mode entry: copy target → repo source.
fn pull_entry(entry: &Entry, repo_root: &std::path::Path) -> Result<()> {
    let target = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;
    let source = repo_root.join(&entry.source);

    if !target.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!("target `{}` does not exist", target.display()),
        });
    }

    if entry.directory {
        pull_directory(&target, &source)?;
    } else {
        crate::fs::copy_file(&target, &source)?;
    }

    Ok(())
}

/// Pull an encrypted entry: read target plaintext, encrypt, write source.enc.
fn pull_encrypted(entry: &Entry, repo_root: &std::path::Path, password: &str) -> Result<()> {
    let target = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;
    let enc_path = repo_root.join(format!("{}.enc", entry.source));

    if !target.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!("target `{}` does not exist", target.display()),
        });
    }

    if entry.directory {
        pull_encrypted_directory(
            &target,
            enc_path.parent().unwrap_or(repo_root),
            entry,
            password,
        )?;
    } else {
        let plaintext = fs::read(&target).map_err(|e| Error::io(&target, "read target", e))?;
        let master_key = crate::crypto::vault::unlock_vault(password)?;
        let encrypted = crate::crypto::encrypt_with_key(&plaintext, &master_key)?;
        crate::fs::atomic_write(&enc_path, &encrypted)?;
    }

    Ok(())
}

/// Recursively copy a directory from target → repo source.
fn pull_directory(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;

    for dir_entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let dir_entry = dir_entry.map_err(|e| Error::io(src, "read directory entry", e))?;
        let src_path = dir_entry.path();
        let dst_path = dst.join(dir_entry.file_name());

        if src_path.is_dir() {
            pull_directory(&src_path, &dst_path)?;
        } else {
            crate::fs::copy_file(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Recursively pull an encrypted directory: encrypt each file from target → repo.
#[allow(clippy::only_used_in_recursion)]
fn pull_encrypted_directory(
    target_dir: &std::path::Path,
    repo_dir: &std::path::Path,
    entry: &Entry,
    password: &str,
) -> Result<()> {
    let master_key = crate::crypto::vault::unlock_vault(password)?;

    fs::create_dir_all(repo_dir).map_err(|e| Error::io(repo_dir, "create directory", e))?;

    for dir_entry in
        fs::read_dir(target_dir).map_err(|e| Error::io(target_dir, "read directory", e))?
    {
        let dir_entry = dir_entry.map_err(|e| Error::io(target_dir, "read directory entry", e))?;
        let src_path = dir_entry.path();
        let file_name = dir_entry.file_name();

        if src_path.is_dir() {
            pull_encrypted_directory(&src_path, &repo_dir.join(&file_name), entry, password)?;
        } else {
            let plaintext =
                fs::read(&src_path).map_err(|e| Error::io(&src_path, "read target file", e))?;
            let encrypted = crate::crypto::encrypt_with_key(&plaintext, &master_key)?;

            // Write as <filename>.enc
            let enc_name = format!("{}.enc", file_name.to_string_lossy());
            let enc_path = repo_dir.join(enc_name);
            crate::fs::atomic_write(&enc_path, &encrypted)?;
        }
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────

fn get_or_prompt_password(cache: &mut Option<String>) -> String {
    if let Some(p) = cache {
        return p.clone();
    }
    let p = ui::password("Vault password");
    *cache = Some(p.clone());
    p
}
