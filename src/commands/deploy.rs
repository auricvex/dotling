use crate::config::Config;
use crate::error::Result;
use crate::platform;
use crate::{store, ui};

/// Deploy all tracked entries.
pub fn run(dry_run: bool, force: bool) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let config = Config::load(&config_path)?;

    if config.entries.is_empty() {
        ui::info("no entries to deploy");
        ui::hint("add files with `dotling add <path>`");
        return Ok(());
    }

    let mut deployed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    let mut password_cache: Option<String> = None;

    for entry in &config.entries {
        // Skip entries not for this OS
        if !platform::should_deploy(entry.os.as_deref()) {
            skipped += 1;
            continue;
        }

        let state = crate::deploy::check_state(entry, &repo_root, config.settings.method);

        if state == crate::deploy::EntryState::Deployed && !force {
            if !dry_run {
                skipped += 1;
            }
            continue;
        }

        if dry_run {
            let action = match state {
                crate::deploy::EntryState::Missing => "would deploy",
                crate::deploy::EntryState::Broken => "would fix",
                crate::deploy::EntryState::Conflict => {
                    if force {
                        "would overwrite"
                    } else {
                        "conflict (use --force)"
                    }
                }
                crate::deploy::EntryState::Modified => "would redeploy",
                crate::deploy::EntryState::Deployed => "ok",
            };
            ui::info(&format!("{action}: {} → {}", entry.source, entry.target));
            continue;
        }

        let result = if entry.encrypted {
            // Prompt for password once
            let password = if let Some(p) = &password_cache { p.clone() } else {
                let p = ui::password("Vault password");
                password_cache = Some(p.clone());
                p
            };
            crate::deploy::deploy_encrypted(entry, &repo_root, &password)
        } else {
            crate::deploy::deploy_entry(entry, &repo_root, config.settings.method, force)
        };

        match result {
            Ok(()) => {
                ui::success(&format!("{} → {}", entry.source, entry.target));
                deployed += 1;
            }
            Err(e) => {
                ui::error(&format!("{} → {}: {e}", entry.source, entry.target));
                errors += 1;
            }
        }
    }

    if dry_run {
        ui::dim("(dry run — no changes made)");
    } else {
        ui::summary(deployed + skipped, 0, errors);
    }

    Ok(())
}
