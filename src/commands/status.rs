use std::fs;

use crate::config::Config;
use crate::error::Result;
use crate::platform;
use crate::{store, ui};

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
                );
                ok_count += 1;
                continue;
            }

            let state = crate::deploy::check_state(entry, &repo_root, config.settings.method);

            let status = match state {
                crate::deploy::EntryState::Deployed => {
                    ok_count += 1;
                    if entry.encrypted {
                        ui::Status::Encrypted
                    } else {
                        ui::Status::Ok
                    }
                }
                crate::deploy::EntryState::Modified => {
                    warning_count += 1;
                    ui::Status::Modified
                }
                crate::deploy::EntryState::Missing => {
                    error_count += 1;
                    ui::Status::Missing
                }
                crate::deploy::EntryState::Broken => {
                    error_count += 1;
                    ui::Status::Broken
                }
                crate::deploy::EntryState::Conflict => {
                    warning_count += 1;
                    ui::Status::Conflict
                }
            };

            ui::status_line(&status, &entry.source, &entry.target);

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
