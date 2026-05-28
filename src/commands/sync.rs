use std::{fs, path::Path};

use crate::{
    backup,
    config::{Config, DeployMethod, Entry},
    deploy::EntryState,
    error::{Error, Result},
    fingerprint::{FingerprintStore, WhichSide},
    merge, platform, store,
    template::RenderContext,
    ui,
    vars::VarStore,
};

// ── Conflict origin ───────────────────────────────────────────────

/// Why a conflict was detected for this entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConflictOrigin {
    /// Target exists but has never been tracked by dotling on this machine.
    /// The user may have had the file before ever running dotling.
    FirstSeen,
    /// Both the repo source and the actual target were modified since the
    /// last sync — genuine divergence.
    BothModified,
    /// Files differ but modification times are identical; we cannot determine
    /// which is newer.
    TimestampTie,
}

impl ConflictOrigin {
    fn label(self) -> &'static str {
        match self {
            Self::FirstSeen => "first-seen",
            Self::BothModified => "both-modified",
            Self::TimestampTie => "ambiguous timestamp",
        }
    }
}

// ── Direction resolved per entry ──────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAction {
    /// Already in sync — nothing to do.
    Ok,
    /// Push from repo → actual (deploy direction).
    Push,
    /// Pull from actual → repo.
    Pull,
    /// Both sides differ and need user input or a flag to resolve.
    Conflict(ConflictOrigin),
    /// Symlink is missing or broken (always a Push).
    FixSymlink,
}

// ── Public entry point ────────────────────────────────────────────

/// Synchronise all tracked entries in both directions.
#[allow(clippy::too_many_lines)]
#[allow(clippy::fn_params_excessive_bools)]
#[allow(clippy::too_many_arguments)]
pub fn run(
    dry_run: bool,
    force: bool,
    prefer_actual: bool,
    no_interactive: bool,
    always_backup: bool,
    allow_hooks: bool,
    no_hooks: bool,
) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let config = Config::load(&config_path)?;

    if config.entries.is_empty() {
        ui::info("no entries to sync");
        ui::hint("add files with `dotling add <path>`");
        return Ok(());
    }

    let mut hook_session = crate::hooks::HookSession::new(allow_hooks, no_hooks);

    // ── Global Before Hook ───────────────────────────────────────
    if let Some(ref before) = config.hooks.before {
        hook_session.run_hook(
            before,
            "global_before",
            &repo_root,
            dry_run,
            no_interactive,
            None,
            None,
        )?;
    }

    let mut pushed = 0usize;
    let mut pulled = 0usize;
    let mut merged = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    let mut password_cache: Option<String> = None;

    // Load the fingerprint store.
    let fp_path = store::fingerprint_path()?;
    let mut fp_store = FingerprintStore::load(fp_path);
    let mut fp_dirty = false;

    // ── Template bootstrap — first-sync prompt ────────────────────
    // Load local var store once; we may update it during bootstrap.
    let mut var_store = VarStore::load().unwrap_or_default();

    if !dry_run && !no_interactive {
        let repo_root_str = repo_root.to_string_lossy().into_owned();
        let ctx = RenderContext::new(&repo_root_str, &config.vars, &var_store.as_pairs());
        let missing = crate::commands::vars::collect_missing_vars(&config, &repo_root, &ctx);
        if !missing.is_empty() {
            let any_set =
                crate::commands::vars::bootstrap_prompt(&missing, &config.vars, &mut var_store);
            if any_set {
                if let Err(e) = var_store.save() {
                    ui::warning(&format!("could not save vars.toml: {e}"));
                }
            }
        }
    }

    for entry in &config.entries {
        // Skip entries not meant for this OS.
        if !platform::should_deploy(entry.os.as_deref()) {
            skipped += 1;
            continue;
        }

        // ── Template entries: always render and deploy — skip conflict logic
        if entry.template {
            if dry_run {
                ui::info(&format!(
                    "would render (template): {} → {}",
                    entry.source, entry.target
                ));
                // Still report would-run hooks in dry-run mode.
                if let Some(ref before) = entry.before {
                    hook_session.run_hook(
                        before,
                        "entry_before",
                        &repo_root,
                        dry_run,
                        no_interactive,
                        Some(entry),
                        Some("push"),
                    )?;
                }
                if let Some(ref after) = entry.after {
                    hook_session.run_hook(
                        after,
                        "entry_after",
                        &repo_root,
                        dry_run,
                        no_interactive,
                        Some(entry),
                        Some("push"),
                    )?;
                }
                skipped += 1;
                continue;
            }

            // ── Entry Before Hook (template) ───────────────────────
            if let Some(ref before) = entry.before {
                if let Err(e) = hook_session.run_hook(
                    before,
                    "entry_before",
                    &repo_root,
                    dry_run,
                    no_interactive,
                    Some(entry),
                    Some("push"),
                ) {
                    ui::error(&format!("before hook for '{}' failed: {e}", entry.source));
                    errors += 1;
                    continue;
                }
            }

            let repo_root_str = repo_root.to_string_lossy().into_owned();
            let template_result = sync_template_entry(
                entry,
                &repo_root,
                &repo_root_str,
                &config.vars,
                &var_store,
                always_backup,
                &mut password_cache,
                &mut fp_store,
                &mut fp_dirty,
            );

            match template_result {
                Ok(rendered) => {
                    if rendered {
                        ui::success(&format!("render {} → {}", entry.source, entry.target));
                        pushed += 1;
                    } else {
                        skipped += 1;
                    }

                    // ── Entry After Hook (template) ────────────────────
                    if let Some(ref after) = entry.after {
                        if let Err(e) = hook_session.run_hook(
                            after,
                            "entry_after",
                            &repo_root,
                            dry_run,
                            no_interactive,
                            Some(entry),
                            Some("push"),
                        ) {
                            ui::error(&format!(
                                "after hook for '{}' failed: {e}",
                                entry.source
                            ));
                            errors += 1;
                        }
                    }
                }
                Err(e) => {
                    ui::error(&format!("{} ↔ {}: {e}", entry.source, entry.target));
                    errors += 1;
                }
            }
            continue;
        }

        let action = resolve_action(
            entry,
            &repo_root,
            config.settings.method,
            prefer_actual,
            &fp_store,
        );

        // ── Dry-run output ─────────────────────────────────────
        if dry_run {
            let label = match action {
                SyncAction::Ok => "ok",
                SyncAction::Push | SyncAction::FixSymlink => "would push (repo → actual)",
                SyncAction::Pull => "would pull (actual → repo)",
                SyncAction::Conflict(o) => match o {
                    ConflictOrigin::FirstSeen => {
                        "conflict — first-seen (local file pre-dates dotling)"
                    }
                    ConflictOrigin::BothModified => "conflict — both sides modified",
                    ConflictOrigin::TimestampTie => "conflict — ambiguous timestamp",
                },
            };
            ui::info(&format!("{label}: {} ↔ {}", entry.source, entry.target));

            // In dry-run, still print the would-run message for hooks if action is not Ok
            if action != SyncAction::Ok {
                let action_str = match action {
                    SyncAction::Push | SyncAction::FixSymlink => "push",
                    SyncAction::Pull => "pull",
                    _ => "unknown",
                };
                if let Some(ref before) = entry.before {
                    hook_session.run_hook(
                        before,
                        "entry_before",
                        &repo_root,
                        dry_run,
                        no_interactive,
                        Some(entry),
                        Some(action_str),
                    )?;
                }
                if let Some(ref after) = entry.after {
                    hook_session.run_hook(
                        after,
                        "entry_after",
                        &repo_root,
                        dry_run,
                        no_interactive,
                        Some(entry),
                        Some(action_str),
                    )?;
                }
            }
            continue;
        }

        // ── Already in sync ────────────────────────────────────
        if action == SyncAction::Ok {
            skipped += 1;
            continue;
        }

        // ── Conflict resolution ────────────────────────────────
        let resolved_action = match action {
            SyncAction::Conflict(origin) => resolve_conflict(
                entry,
                &repo_root,
                origin,
                force,
                prefer_actual,
                no_interactive,
                &mut fp_store,
                &mut fp_dirty,
            )
            .map_or(None, |outcome| match outcome {
                ConflictOutcome::Resolved(a) => Some(a),
                ConflictOutcome::Merged => {
                    merged += 1;
                    None
                }
                ConflictOutcome::Skipped => {
                    skipped += 1;
                    None
                }
            }),
            other => Some(other),
        };

        let Some(final_action) = resolved_action else {
            continue;
        };

        let action_str = match final_action {
            SyncAction::Push | SyncAction::FixSymlink => "push",
            SyncAction::Pull => "pull",
            _ => "unknown",
        };

        // ── Entry Before Hook ──────────────────────────────────
        if let Some(ref before) = entry.before {
            if let Err(e) = hook_session.run_hook(
                before,
                "entry_before",
                &repo_root,
                dry_run,
                no_interactive,
                Some(entry),
                Some(action_str),
            ) {
                ui::error(&format!("before hook for '{}' failed: {e}", entry.source));
                errors += 1;
                continue;
            }
        }

        // ── Execute the resolved action ────────────────────────
        let result = match final_action {
            SyncAction::Push | SyncAction::FixSymlink => {
                if always_backup || force {
                    maybe_backup(entry, &repo_root);
                }
                if entry.encrypted {
                    let password = get_or_prompt_password(&mut password_cache);
                    crate::deploy::deploy_encrypted(entry, &repo_root, &password)
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
            SyncAction::Ok | SyncAction::Conflict(_) => unreachable!(),
        };

        match result {
            Ok(()) => {
                match final_action {
                    SyncAction::Push | SyncAction::FixSymlink => {
                        ui::success(&format!("push {} → {}", entry.source, entry.target));
                        pushed += 1;
                        record_fingerprint_after_push(
                            entry,
                            &repo_root,
                            &mut fp_store,
                            &mut fp_dirty,
                        );
                    }
                    SyncAction::Pull => {
                        ui::success(&format!("pull {} ← {}", entry.source, entry.target));
                        pulled += 1;
                        record_fingerprint_after_pull(
                            entry,
                            &repo_root,
                            &mut fp_store,
                            &mut fp_dirty,
                        );
                    }
                    _ => {}
                }

                // ── Entry After Hook ───────────────────────────────────
                if let Some(ref after) = entry.after {
                    if let Err(e) = hook_session.run_hook(
                        after,
                        "entry_after",
                        &repo_root,
                        dry_run,
                        no_interactive,
                        Some(entry),
                        Some(action_str),
                    ) {
                        ui::error(&format!("after hook for '{}' failed: {e}", entry.source));
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                ui::error(&format!("{} ↔ {}: {e}", entry.source, entry.target));
                errors += 1;
            }
        }
    }

    // ── Global After Hook ────────────────────────────────────────
    if !dry_run {
        if let Some(ref after) = config.hooks.after {
            hook_session.run_hook(
                after,
                "global_after",
                &repo_root,
                dry_run,
                no_interactive,
                None,
                None,
            )?;
        }
    }

    if dry_run {
        ui::dim("(dry run — no changes made)");
    } else {
        if fp_dirty {
            if let Err(e) = fp_store.save() {
                ui::warning(&format!("could not save sync fingerprints: {e}"));
            }
        }

        // Print a rich summary.
        let mut parts = Vec::new();
        if pushed > 0 {
            parts.push(format!("{pushed} pushed"));
        }
        if pulled > 0 {
            parts.push(format!("{pulled} pulled"));
        }
        if merged > 0 {
            parts.push(format!("{merged} merged"));
        }
        if skipped > 0 {
            parts.push(format!("{skipped} skipped"));
        }

        ui::summary(pushed + pulled + merged + skipped, 0, errors);
    }

    Ok(())
}

// ── Conflict resolution ───────────────────────────────────────────

enum ConflictOutcome {
    /// Continue with the given `SyncAction` (Push or Pull).
    Resolved(SyncAction),
    /// A 3-way merge was performed and written to disk.
    Merged,
    /// User chose to skip.
    Skipped,
}

#[allow(clippy::too_many_arguments)]
fn resolve_conflict(
    entry: &Entry,
    repo_root: &Path,
    origin: ConflictOrigin,
    force: bool,
    prefer_actual: bool,
    no_interactive: bool,
    fp_store: &mut FingerprintStore,
    fp_dirty: &mut bool,
) -> Result<ConflictOutcome> {
    // Non-interactive fast paths.
    if force {
        // Repo wins — back up first.
        maybe_backup(entry, repo_root);
        return Ok(ConflictOutcome::Resolved(SyncAction::Push));
    }
    if prefer_actual {
        return Ok(ConflictOutcome::Resolved(SyncAction::Pull));
    }
    if no_interactive {
        ui::warning(&format!(
            "conflict ({}): {} ↔ {} — skipped (use --force or --prefer-actual to resolve)",
            origin.label(),
            entry.source,
            entry.target,
        ));
        return Ok(ConflictOutcome::Skipped);
    }

    // Interactive resolution.
    let Ok(target) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) else {
        return Ok(ConflictOutcome::Resolved(SyncAction::Push));
    };
    let source_path = repo_root.join(&entry.source);

    // For encrypted entries we only offer keep/repo/skip.
    let supports_merge =
        !entry.encrypted && !entry.directory && source_path.exists() && target.exists();

    ui::conflict_header(origin.label(), &entry.source, &entry.target);

    loop {
        let choice = ui::conflict_prompt(supports_merge);
        match choice {
            ui::ConflictChoice::ShowDiff => {
                if source_path.exists() && target.exists() {
                    if let (Ok(repo_txt), Ok(act_txt)) = (
                        fs::read_to_string(&source_path),
                        fs::read_to_string(&target),
                    ) {
                        ui::print_diff(&entry.source, &entry.target, &repo_txt, &act_txt);
                    } else {
                        ui::warning("could not read files as UTF-8 for diff");
                    }
                }
                // loop continues automatically to re-prompt
            }

            ui::ConflictChoice::KeepLocal => {
                return Ok(ConflictOutcome::Resolved(SyncAction::Pull));
            }

            ui::ConflictChoice::UseRepo => {
                // Backup before overwriting.
                if target.exists() {
                    match backup::backup(&target, &entry.source) {
                        Ok(p) => ui::backup_notice(&p),
                        Err(e) => ui::warning(&format!("could not create backup: {e}")),
                    }
                }
                return Ok(ConflictOutcome::Resolved(SyncAction::Push));
            }

            ui::ConflictChoice::Merge => {
                // 3-way merge using the on-disk snapshot as base.
                match perform_three_way_merge(entry, repo_root, &target, fp_store, fp_dirty) {
                    Ok(()) => return Ok(ConflictOutcome::Merged),
                    Err(e) => {
                        ui::warning(&format!("merge failed: {e} — try another option"));
                        // loop continues to re-prompt
                    }
                }
            }

            ui::ConflictChoice::Skip => {
                ui::dim(&format!("  skipped: {} ↔ {}", entry.source, entry.target));
                return Ok(ConflictOutcome::Skipped);
            }
        }
    }
}

/// Perform a 3-way merge between the snapshot (base), repo source (ours),
/// and actual file (theirs). Writes the merged result to the actual path and
/// updates the repo source with the merge result (or conflict markers).
fn perform_three_way_merge(
    entry: &Entry,
    repo_root: &Path,
    target: &Path,
    fp_store: &mut FingerprintStore,
    fp_dirty: &mut bool,
) -> Result<()> {
    let source_path = repo_root.join(&entry.source);
    let snap_path = store::snapshot_path(&entry.source)?;

    let repo_text = fs::read_to_string(&source_path)
        .map_err(|e| Error::io(&source_path, "read repo source", e))?;
    let actual_text =
        fs::read_to_string(target).map_err(|e| Error::io(target, "read actual file", e))?;

    // Load the base snapshot if it exists; otherwise use empty string as base
    // (effectively a 2-way merge — not ideal, but still useful).
    let base_text = if snap_path.exists() {
        fs::read_to_string(&snap_path).unwrap_or_default()
    } else {
        String::new()
    };

    let result = merge::three_way_merge(&base_text, &repo_text, &actual_text, "repo", "actual");

    // Back up the actual file before writing the merge result.
    if target.exists() {
        match backup::backup(target, &entry.source) {
            Ok(p) => ui::backup_notice(&p),
            Err(e) => ui::warning(&format!("could not create backup: {e}")),
        }
    }

    // Write merge result to the actual file.
    crate::fs::atomic_write(target, result.content.as_bytes())?;

    // Mirror the merge result back to the repo source so both sides are in sync.
    crate::fs::atomic_write(&source_path, result.content.as_bytes())?;

    // Save the merged content as the new snapshot base.
    if let Some(parent) = snap_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::io(parent, "create snapshot directory", e))?;
    }
    crate::fs::atomic_write(&snap_path, result.content.as_bytes())?;

    // Update fingerprints.
    if fp_store
        .record_plain(&entry.source, &source_path, target)
        .is_ok()
    {
        *fp_dirty = true;
    }

    if result.has_conflicts {
        ui::merge_conflict_notice(result.conflict_count, target);
    } else {
        ui::merge_clean_notice(target);
    }

    Ok(())
}

// ── Action resolution ─────────────────────────────────────────────

fn resolve_action(
    entry: &Entry,
    repo_root: &Path,
    default_method: DeployMethod,
    prefer_actual: bool,
    fp_store: &FingerprintStore,
) -> SyncAction {
    let method = entry.method.unwrap_or(default_method);

    let Ok(target) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) else {
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
            if entry.encrypted {
                resolve_encrypted_action(entry, repo_root, &target, prefer_actual)
            } else {
                resolve_copy_action(entry, repo_root, &target, fp_store)
            }
        }
    }
}

/// Resolve sync direction for a plain copy-mode entry.
///
/// Priority:
/// 1. Content-identical → Ok.
/// 2. Consult fingerprint `who_changed()` for a deterministic answer.
/// 3. Fall back to mtime comparison.
/// 4. Mark as conflict with an appropriate origin.
fn resolve_copy_action(
    entry: &Entry,
    repo_root: &Path,
    target: &Path,
    fp_store: &FingerprintStore,
) -> SyncAction {
    let source = repo_root.join(&entry.source);
    let source_exists = source.exists();
    let target_exists = target.exists() && !crate::fs::is_symlink(target);

    match (source_exists, target_exists) {
        (false, false) => SyncAction::Ok,
        (true, false) => SyncAction::Push,
        (false, true) => SyncAction::Pull,
        (true, true) => {
            // Fast path: identical content.
            if matches!(crate::fs::files_identical(&source, target), Ok(true)) {
                // Ensure fingerprint is recorded even if already in sync.
                return SyncAction::Ok;
            }

            // Use fingerprint baseline to determine which side changed.
            match fp_store.who_changed(&entry.source, &source, target) {
                WhichSide::Neither => SyncAction::Ok,
                WhichSide::RepoOnly => SyncAction::Push,
                WhichSide::ActualOnly => SyncAction::Pull,
                WhichSide::Both => SyncAction::Conflict(ConflictOrigin::BothModified),

                // No fingerprint recorded yet — target may have pre-existed dotling.
                WhichSide::Unknown => {
                    // Target exists but was never tracked here: treat as first-seen.
                    SyncAction::Conflict(ConflictOrigin::FirstSeen)
                }
            }
        }
    }
}

/// Resolve sync direction for an encrypted entry (mtime-only, no decryption).
fn resolve_encrypted_action(
    entry: &Entry,
    repo_root: &Path,
    target: &Path,
    _prefer_actual: bool,
) -> SyncAction {
    let enc_path = if entry.directory {
        repo_root.join(&entry.source)
    } else if entry.template {
        // For encrypted templates the .enc suffix is already baked into entry.source
        // (e.g. "shell/gitconfig.dtmpl.enc") — do NOT append another .enc.
        repo_root.join(&entry.source)
    } else {
        repo_root.join(format!("{}.enc", entry.source))
    };
    let enc_exists = enc_path.exists();
    let target_exists = target.exists() && !crate::fs::is_symlink(target);

    match (enc_exists, target_exists) {
        (false, false) => SyncAction::Ok,
        (true, false) => SyncAction::Push,
        (false, true) => SyncAction::Pull,
        (true, true) => {
            if entry.directory {
                match (latest_mtime(&enc_path), latest_mtime(target)) {
                    (Ok(et), Ok(tt)) => match tt.cmp(&et) {
                        std::cmp::Ordering::Greater => SyncAction::Pull,
                        std::cmp::Ordering::Less => SyncAction::Push,
                        std::cmp::Ordering::Equal => SyncAction::Ok,
                    },
                    _ => SyncAction::Conflict(ConflictOrigin::TimestampTie),
                }
            } else {
                match (enc_path.metadata(), target.metadata()) {
                    (Ok(em), Ok(tm)) => match (em.modified(), tm.modified()) {
                        (Ok(et), Ok(tt)) => match tt.cmp(&et) {
                            std::cmp::Ordering::Greater => SyncAction::Pull,
                            std::cmp::Ordering::Less => SyncAction::Push,
                            std::cmp::Ordering::Equal => SyncAction::Ok,
                        },
                        _ => SyncAction::Conflict(ConflictOrigin::TimestampTie),
                    },
                    _ => SyncAction::Push,
                }
            }
        }
    }
}

fn latest_mtime(dir: &Path) -> Result<std::time::SystemTime> {
    let mut latest = std::time::SystemTime::UNIX_EPOCH;
    if !dir.exists() {
        return Ok(latest);
    }
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();
        let mtime = if path.is_dir() {
            latest_mtime(&path)?
        } else {
            path.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        };
        if mtime > latest {
            latest = mtime;
        }
    }
    Ok(latest)
}

// ── Pull (actual → repo) ──────────────────────────────────────────

/// Pull a plain copy-mode entry: copy target → repo source.
fn pull_entry(entry: &Entry, repo_root: &Path) -> Result<()> {
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
fn pull_encrypted(entry: &Entry, repo_root: &Path, password: &str) -> Result<()> {
    let target = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;

    if !target.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!("target `{}` does not exist", target.display()),
        });
    }

    if entry.directory {
        let repo_dir = repo_root.join(&entry.source);
        pull_encrypted_directory(&target, &repo_dir, entry, password)?;
    } else {
        // For encrypted templates the .enc suffix is already part of entry.source
        // (e.g. "shell/gitconfig.dtmpl.enc"); for plain encrypted files we append it.
        let enc_path = if entry.template {
            repo_root.join(&entry.source)
        } else {
            repo_root.join(format!("{}.enc", entry.source))
        };
        let plaintext = fs::read(&target).map_err(|e| Error::io(&target, "read target", e))?;
        let master_key = crate::crypto::vault::unlock_vault(password)?;
        let encrypted = crate::crypto::encrypt_with_key(&plaintext, &master_key)?;
        crate::fs::atomic_write(&enc_path, &encrypted)?;
    }

    Ok(())
}

/// Recursively copy a directory from target → repo source.
fn pull_directory(src: &Path, dst: &Path) -> Result<()> {
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
    target_dir: &Path,
    repo_dir: &Path,
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

            let enc_name = format!("{}.enc", file_name.to_string_lossy());
            let enc_path = repo_dir.join(enc_name);
            crate::fs::atomic_write(&enc_path, &encrypted)?;
        }
    }

    Ok(())
}

// ── Fingerprint recording ─────────────────────────────────────────

fn record_fingerprint_after_push(
    entry: &Entry,
    repo_root: &Path,
    fp_store: &mut FingerprintStore,
    fp_dirty: &mut bool,
) {
    if entry.encrypted {
        let enc_path = if entry.directory {
            repo_root.join(&entry.source)
        } else {
            repo_root.join(format!("{}.enc", entry.source))
        };
        if let Ok(target_path) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) {
            if fp_store
                .record(&entry.source, &enc_path, &target_path)
                .is_ok()
            {
                *fp_dirty = true;
            }
        }
    } else if !entry.directory {
        // Copy-mode plain file: record source + target hashes + save snapshot.
        let source_path = repo_root.join(&entry.source);
        if let Ok(target_path) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) {
            if fp_store
                .record_plain(&entry.source, &source_path, &target_path)
                .is_ok()
            {
                *fp_dirty = true;
                // Save snapshot (best-effort).
                save_snapshot(entry, &target_path);
            }
        }
    }
}

fn record_fingerprint_after_pull(
    entry: &Entry,
    repo_root: &Path,
    fp_store: &mut FingerprintStore,
    fp_dirty: &mut bool,
) {
    if entry.encrypted {
        let enc_path = if entry.directory {
            repo_root.join(&entry.source)
        } else {
            repo_root.join(format!("{}.enc", entry.source))
        };
        if let Ok(target_path) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) {
            if fp_store
                .record(&entry.source, &enc_path, &target_path)
                .is_ok()
            {
                *fp_dirty = true;
            }
        }
    } else if !entry.directory {
        let source_path = repo_root.join(&entry.source);
        if let Ok(target_path) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) {
            if fp_store
                .record_plain(&entry.source, &source_path, &target_path)
                .is_ok()
            {
                *fp_dirty = true;
                save_snapshot(entry, &target_path);
            }
        }
    }
}

/// Write a plaintext snapshot of `target_path` to `~/.dotling/snapshots/<source>`.
fn save_snapshot(entry: &Entry, target_path: &Path) {
    if let Ok(snap_path) = store::snapshot_path(&entry.source) {
        if let Some(parent) = snap_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = std::fs::read(target_path) {
            let _ = crate::fs::atomic_write(&snap_path, &content);
        }
    }
}

// ── Backup helper ─────────────────────────────────────────────────

/// Silently backup the actual file if it exists.
fn maybe_backup(entry: &Entry, repo_root: &Path) {
    if let Ok(target) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) {
        if target.exists() {
            if entry.directory {
                match backup::backup_dir(&target, &entry.source) {
                    Ok(p) => ui::backup_notice(&p),
                    Err(e) => ui::warning(&format!("could not create backup: {e}")),
                }
            } else {
                match backup::backup(&target, &entry.source) {
                    Ok(p) => ui::backup_notice(&p),
                    Err(e) => ui::warning(&format!("could not create backup: {e}")),
                }
            }
        }
    }
    // Silence unused warning — push_encrypted is only reachable through the old path.
    let _ = &repo_root;
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

// ── Template sync ─────────────────────────────────────────────────

/// Render and deploy a single template entry.
///
/// Returns `Ok(true)` if the rendered file was written, `Ok(false)` if it was
/// already up-to-date (fingerprint matched).
#[allow(clippy::too_many_arguments)]
fn sync_template_entry(
    entry: &Entry,
    repo_root: &Path,
    repo_root_str: &str,
    config_vars: &[(String, String)],
    var_store: &VarStore,
    always_backup: bool,
    password_cache: &mut Option<String>,
    fp_store: &mut FingerprintStore,
    fp_dirty: &mut bool,
) -> Result<bool> {
    let source_path = repo_root.join(&entry.source);

    if !source_path.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!(
                "template source not found in repo: {}",
                source_path.display()
            ),
        });
    }

    // Read template text (decrypt if encrypted)
    let template_text: String = if entry.encrypted {
        let password = get_or_prompt_password(password_cache);
        let mk = crate::crypto::vault::unlock_vault(&password)?;
        let encrypted = fs::read(&source_path)
            .map_err(|e| Error::io(&source_path, "read encrypted template", e))?;
        let plaintext_bytes = crate::crypto::decrypt_with_key(&encrypted, &mk)?;
        String::from_utf8(plaintext_bytes).map_err(|_| Error::Template {
            source: entry.source.clone(),
            message: "encrypted template is not valid UTF-8".into(),
        })?
    } else {
        fs::read_to_string(&source_path).map_err(|e| Error::io(&source_path, "read template", e))?
    };

    // Build render context
    let ctx =
        crate::template::RenderContext::new(repo_root_str, config_vars, &var_store.as_pairs());

    // Render
    let rendered = crate::template::render(&template_text, &ctx, &entry.source)?;

    // Expand target path
    let target_path = crate::path::expand_tilde(std::path::Path::new(&entry.target))?;

    // Check fingerprint of rendered content against current deployed file
    let rendered_hash = blake3_hash(rendered.as_bytes());
    let current_hash = if target_path.exists() {
        fs::read(&target_path)
            .map(|b| blake3_hash(&b))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    if rendered_hash == current_hash && !current_hash.is_empty() {
        // Already up-to-date
        return Ok(false);
    }

    // Optionally backup existing target
    if always_backup && target_path.exists() {
        match backup::backup(&target_path, &entry.source) {
            Ok(p) => ui::backup_notice(&p),
            Err(e) => ui::warning(&format!("could not create backup: {e}")),
        }
    }

    // Write rendered output
    crate::fs::atomic_write(&target_path, rendered.as_bytes())?;

    // Apply permissions if set
    if let Some(perms) = entry.permissions {
        crate::fs::set_permissions(&target_path, perms)?;
    }

    // Record fingerprint (hash of rendered content)
    // We store the rendered output hash; key = entry.source
    if fp_store
        .record_plain(&entry.source, &source_path, &target_path)
        .is_ok()
    {
        *fp_dirty = true;
    }

    Ok(true)
}

/// Compute a simple hash of `data` using blake2b (already a dependency).
/// Returns raw digest bytes.
fn blake3_hash(data: &[u8]) -> Vec<u8> {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}
