use crate::config::Config;
use crate::error::Result;
use crate::{store, ui};

/// Audit repository health.
#[allow(clippy::too_many_lines)]
pub fn run() -> Result<()> {
    ui::header("Doctor");

    let mut ok = 0usize;
    let mut warnings = 0usize;
    let mut errors = 0usize;

    // 1. Check repo root
    if let Some(repo_root) = store::get_repo_root()? {
        if repo_root.exists() {
            ui::success(&format!("repo: {}", repo_root.display()));
            ok += 1;
        } else {
            ui::error(&format!(
                "repo directory missing: {}",
                repo_root.display()
            ));
            // errors += 1 — early return so we don't need to track it
            ui::summary(0, 0, 1);
            return Ok(());
        }

        // 2. Check config file
        let config_path = store::config_path(&repo_root);
        if config_path.exists() {
            match Config::load(&config_path) {
                Ok(config) => {
                    ui::success(&format!(
                        "config: {} entries",
                        config.entries.len()
                    ));
                    ok += 1;

                    // 3. Check each entry
                    for entry in &config.entries {
                        let state = crate::deploy::check_state(
                            entry,
                            &repo_root,
                            config.settings.method,
                        );

                        let source_path = if entry.encrypted {
                            repo_root.join(format!("{}.enc", entry.source))
                        } else {
                            repo_root.join(&entry.source)
                        };

                        // Check source exists in repo
                        if !source_path.exists() {
                            ui::error(&format!(
                                "missing source: {}",
                                entry.source
                            ));
                            errors += 1;
                            continue;
                        }

                        match state {
                            crate::deploy::EntryState::Deployed => {
                                ok += 1;
                            }
                            crate::deploy::EntryState::Modified => {
                                ui::warning(&format!(
                                    "modified: {} → {}",
                                    entry.source, entry.target
                                ));
                                warnings += 1;
                            }
                            crate::deploy::EntryState::Missing => {
                                ui::error(&format!(
                                    "not deployed: {} → {}",
                                    entry.source, entry.target
                                ));
                                errors += 1;
                            }
                            crate::deploy::EntryState::Broken => {
                                ui::error(&format!(
                                    "broken link: {} → {}",
                                    entry.source, entry.target
                                ));
                                errors += 1;
                            }
                            crate::deploy::EntryState::Conflict => {
                                ui::warning(&format!(
                                    "conflict: {} → {}",
                                    entry.source, entry.target
                                ));
                                warnings += 1;
                            }
                        }
                    }

                    // 4. Check for orphan files in repo
                    check_orphans(&repo_root, &config);
                }
                Err(e) => {
                    ui::error(&format!("config error: {e}"));
                    errors += 1;
                }
            }
        } else {
            ui::error("config file (dotling.toml) not found");
            errors += 1;
        }

        // 5. Check git
        if repo_root.join(".git").exists() {
            ui::success("git: initialized");
            ok += 1;
        } else {
            ui::warning("git: not initialized");
            warnings += 1;
        }

        // 6. Check vault
        if crate::crypto::vault::vault_exists() {
            ui::success("vault: initialized");
            ok += 1;
        } else {
            ui::info("vault: not initialized (optional)");
        }
    } else {
        ui::error("no repo found — run `dotling init <path>`");
        errors += 1;
    }

    ui::summary(ok, warnings, errors);

    Ok(())
}

/// Check for files in the repo that aren't tracked in the config.
fn check_orphans(repo_root: &std::path::Path, config: &Config) {
    let Ok(files) = crate::fs::walk_dir(repo_root, false) else {
        return;
    };

    for file in files {
        let Ok(rel) = file.strip_prefix(repo_root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy();

        // Skip known files
        if rel_str == "dotling.toml" || rel_str.starts_with(".git") {
            continue;
        }

        // Check if any entry tracks this file
        let is_tracked = config.entries.iter().any(|e| {
            let source = &e.source;
            let enc_source = format!("{source}.enc");
            rel_str == *source
                || rel_str == enc_source
                || rel_str.starts_with(&format!("{source}/"))
        });

        if !is_tracked {
            ui::warning(&format!("orphan: {rel_str}"));
        }
    }
}
