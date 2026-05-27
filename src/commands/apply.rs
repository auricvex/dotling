/// Apply (re-deploy) tracked entries.
///
/// Re-deploys missing and broken entries. Skips conflicts and modified
/// entries. Supports `--dry-run` mode. If a source file is missing from
/// the repo, prints a red error and continues.
use std::path::Path;

use crate::{
    config::Config,
    error::Result,
    linker::{DeployResult, EntryStatus, Linker},
    printer::Printer,
    repo,
};

/// Runs the `apply` command.
pub fn run(printer: &Printer, dry_run: bool) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let config = Config::load(&repo_root)?;
    let linker = Linker::new(repo_root.clone());

    if config.entries.is_empty() {
        printer.annotation("No tracked entries.");
        return Ok(());
    }

    let mut deployed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for entry in config.active_entries() {
        let dest_path = repo::src_to_dest_path(&entry.dest)?;
        let src_path = repo_root.join(&entry.src);

        // Check if source exists in repo
        if !src_path.exists() {
            printer.error_line("missing", Path::new(&entry.src));
            printer.annotation("  source file missing from repo");
            errors += 1;
            continue;
        }

        let status = linker.check_entry(entry)?;
        match status {
            EntryStatus::Ok => {
                printer.ok("ok", &dest_path);
                skipped += 1;
            }
            EntryStatus::Modified => {
                printer.skipped("modified", &dest_path);
                printer.hint("  use --force with sync to overwrite");
                skipped += 1;
            }
            EntryStatus::Conflict => {
                printer.skipped("conflict", &dest_path);
                printer.hint("  unmanaged file at destination");
                skipped += 1;
            }
            EntryStatus::Missing | EntryStatus::BrokenSymlink => {
                if dry_run {
                    printer.action("would deploy", &dest_path);
                    deployed += 1;
                } else {
                    // Remove broken symlink before re-deploying
                    if dest_path.is_symlink() {
                        let _ = std::fs::remove_file(&dest_path);
                    }
                    match linker.deploy_entry(entry, false) {
                        Ok(DeployResult::Created) => {
                            printer.ok("deployed", &dest_path);
                            deployed += 1;
                        }
                        Ok(_) => {
                            skipped += 1;
                        }
                        Err(e) => {
                            printer.warn("error", &format!("{}: {e}", dest_path.display()));
                            errors += 1;
                        }
                    }
                }
            }
        }
    }

    if dry_run {
        printer.annotation(&format!(
            "\ndry run: {deployed} would be deployed, {skipped} skipped, {errors} errors"
        ));
    } else {
        printer.annotation(&format!(
            "\n{deployed} deployed, {skipped} skipped, {errors} errors"
        ));
    }

    Ok(())
}
