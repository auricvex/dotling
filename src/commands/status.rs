use std::{fs, path::Path};

use ui::SyncBadge;

use crate::{config::Config, error::Result, fingerprint::FingerprintStore, platform, store, ui};

/// Show status of all tracked entries.
#[allow(clippy::too_many_lines)]
pub fn run(show_diff: bool) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let config = Config::load(&config_path)?;

    if config.entries.is_empty() {
        ui::info("no entries tracked");
        ui::hint("add files with `dotling add <path>`");
        return Ok(());
    }

    let mut ok_count = 0usize;
    let mut warning_count = 0usize;
    let mut error_count = 0usize;

    // Load the fingerprint store for encrypted entry sync-state checks.
    let fp_store = store::fingerprint_path().map_or_else(
        |_| FingerprintStore::load(std::path::PathBuf::new()),
        FingerprintStore::load,
    );

    ui::header("Tracked entries");

    // Group entries by category (first path component of source)
    let mut categories: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();

    for (i, entry) in config.entries.iter().enumerate() {
        let category = entry
            .source
            .split('/')
            .next()
            .unwrap_or("other")
            .to_string();
        categories.entry(category).or_default().push(i);
    }

    for (category, indices) in &categories {
        println!("\n  {category}/");

        for &idx in indices {
            let entry = &config.entries[idx];

            // Skip check for wrong OS
            if !platform::should_deploy(entry.os.as_deref()) {
                let status = ui::Status::Ok;
                let suffix = format!(" ({})", entry.os.as_deref().unwrap_or("all"));
                ui::status_line(
                    &status,
                    &entry.source,
                    &format!("{}{}", entry.target, suffix),
                    SyncBadge::InSync,
                );
                ok_count += 1;
                continue;
            }

            let state = crate::deploy::check_state(entry, &repo_root, config.settings.method);

            let (status, badge) = match state {
                crate::deploy::EntryState::Deployed => {
                    ok_count += 1;
                    if entry.template {
                        // Template: check if deployed file matches the last rendered fingerprint.
                        // We use the source path (template file) and target path.
                        let source_path = repo_root.join(&entry.source);
                        let badge = match crate::path::expand_tilde(Path::new(&entry.target)) {
                            Ok(target_path) => {
                                match fp_store.is_in_sync(&entry.source, &source_path, &target_path)
                                {
                                    Some(true) => SyncBadge::InSync,
                                    Some(false) => {
                                        warning_count += 1;
                                        ok_count -= 1;
                                        SyncBadge::NeedsSync
                                    }
                                    None => {
                                        // Never synced — treat as needs sync
                                        warning_count += 1;
                                        ok_count -= 1;
                                        SyncBadge::NeedsSync
                                    }
                                }
                            }
                            Err(_) => SyncBadge::InSync,
                        };
                        (ui::Status::Template, badge)
                    } else if entry.encrypted {
                        let enc_path = repo_root.join(&entry.source);
                        let badge = match crate::path::expand_tilde(Path::new(&entry.target)) {
                            Ok(target_path) => {
                                match fp_store.is_in_sync(&entry.source, &enc_path, &target_path) {
                                    Some(true) => SyncBadge::InSync,
                                    Some(false) => {
                                        // Counts as a warning — something drifted.
                                        warning_count += 1;
                                        ok_count -= 1;
                                        SyncBadge::NeedsSync
                                    }
                                    None => {
                                        // Never synced via dotling — conservative.
                                        warning_count += 1;
                                        ok_count -= 1;
                                        SyncBadge::NeedsSync
                                    }
                                }
                            }
                            Err(_) => SyncBadge::InSync,
                        };
                        (ui::Status::Encrypted, badge)
                    } else {
                        (ui::Status::Ok, SyncBadge::InSync)
                    }
                }
                crate::deploy::EntryState::Modified => {
                    warning_count += 1;
                    (ui::Status::Modified, SyncBadge::HasDiff)
                }
                crate::deploy::EntryState::Missing => {
                    error_count += 1;
                    (ui::Status::Missing, SyncBadge::NeedsSync)
                }
                crate::deploy::EntryState::Broken => {
                    error_count += 1;
                    (ui::Status::Broken, SyncBadge::NeedsSync)
                }
                crate::deploy::EntryState::Conflict => {
                    warning_count += 1;
                    (ui::Status::Conflict, SyncBadge::NeedsSync)
                }
            };

            ui::status_line(&status, &entry.source, &entry.target, badge);

            // Show diff for modified entries if requested
            if show_diff && state == crate::deploy::EntryState::Modified {
                let source_path = repo_root.join(&entry.source);
                let target_path = crate::path::expand_tilde(std::path::Path::new(&entry.target));

                if let Ok(target_path) = target_path {
                    if let (Ok(source_content), Ok(target_content)) = (
                        fs::read_to_string(&source_path),
                        fs::read_to_string(&target_path),
                    ) {
                        println!();
                        ui::print_diff(
                            &format!("repo:{}", entry.source),
                            &entry.target,
                            &source_content,
                            &target_content,
                        );
                        println!();
                    }
                }
            }
        }
    }

    ui::summary(ok_count, warning_count, error_count);

    Ok(())
}
