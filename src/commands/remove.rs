use std::fs;

use crate::{config::Config, error::Result, store, ui};

/// Remove entries from tracking.
pub fn run(entries: &[String], purge: bool) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let mut config = Config::load(&config_path)?;

    let mut removed = 0usize;
    let mut errors = 0usize;

    for query in entries {
        // Find by source or target
        let entry = config.find_entry(query).cloned();

        if let Some(entry) = entry {
            // Undeploy
            if let Err(e) = crate::deploy::undeploy_entry(&entry) {
                ui::warning(&format!("could not undeploy `{}`: {e}", entry.source));
            }

            // Try to restore original file from repo
            let target = crate::path::expand_tilde(std::path::Path::new(&entry.target));
            let source = repo_root.join(&entry.source);

            if let Ok(target) = target {
                if !target.exists() && source.exists() && !entry.encrypted {
                    if entry.directory {
                        copy_dir_recursive(&source, &target).ok();
                    } else {
                        crate::fs::copy_file(&source, &target).ok();
                    }
                }
            }

            // Purge source from repo if requested
            if purge {
                let repo_source = if entry.encrypted {
                    repo_root.join(format!("{}.enc", entry.source))
                } else {
                    repo_root.join(&entry.source)
                };

                if repo_source.exists() {
                    if repo_source.is_dir() {
                        fs::remove_dir_all(&repo_source).ok();
                    } else {
                        fs::remove_file(&repo_source).ok();
                    }
                }

                // Clean up empty parent dirs
                crate::fs::cleanup_empty_parents(&repo_source, &repo_root).ok();
            }

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
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> crate::error::Result<()> {
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
