/// Sync dotfiles with the remote repository.
///
/// Requires a git remote. Optionally pushes first, then pulls with rebase
/// and re-applies all entries: fixes missing/broken links, skips modified
/// unless `--force`. Supports `--dry-run`.
use crate::{
    config::Config,
    error::{DotlingError, Result},
    git::{Git, PullResult},
    linker::{DeployResult, EntryStatus, Linker},
    printer::Printer,
    repo,
};

/// Runs the `sync` command.
pub fn run(printer: &Printer, push_first: bool, force: bool, dry_run: bool) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let git = Git::new(repo_root.clone());

    if !git.has_remote()? {
        return Err(DotlingError::NoRemote);
    }

    if push_first {
        push_changes(printer, &git, &repo_root)?;
    }

    if dry_run {
        printer.annotation("dry run: skipping pull");
    } else {
        pull_changes(printer, &git, &repo_root)?;
    }

    apply_entries(printer, &repo_root, force, dry_run)
}

/// Pushes local changes before syncing.
fn push_changes(printer: &Printer, _git: &Git, _repo_root: &std::path::Path) -> Result<()> {
    crate::commands::push::run(printer, Some("dotling: update dotfiles"))
}

/// Pulls remote changes with rebase.
fn pull_changes(printer: &Printer, git: &Git, repo_root: &std::path::Path) -> Result<()> {
    printer.action("pull", repo_root);
    match git.pull_rebase()? {
        PullResult::UpToDate => {
            printer.ok("up-to-date", repo_root);
        }
        PullResult::Updated(count) => {
            printer.ok("updated", repo_root);
            printer.annotation(&format!("  {count} file(s) changed"));
        }
        PullResult::Conflict => {
            printer
                .error_msg("Rebase conflict detected. Resolve manually and run `dotling apply`.");
        }
    }
    Ok(())
}

/// Re-applies all config entries after a pull.
fn apply_entries(
    printer: &Printer,
    repo_root: &std::path::Path,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let config = Config::load(repo_root)?;
    let linker = Linker::new(repo_root.to_path_buf());

    let mut ok_count = 0usize;
    let mut fixed_count = 0usize;
    let mut skip_count = 0usize;

    for entry in config.active_entries() {
        let dest_path = repo::src_to_dest_path(&entry.dest)?;
        let src_path = repo_root.join(&entry.src);

        if !src_path.exists() {
            printer.error_line("missing", std::path::Path::new(&entry.src));
            skip_count += 1;
            continue;
        }

        match check_and_fix(printer, &linker, entry, &dest_path, force, dry_run)? {
            FixResult::Ok => ok_count += 1,
            FixResult::Fixed => fixed_count += 1,
            FixResult::Skipped => skip_count += 1,
        }
    }

    printer.summary(ok_count + fixed_count, skip_count, 0);

    if fixed_count > 0 {
        printer.success(&format!("Synced — {fixed_count} entries fixed."));
    } else {
        printer.success("Synced — everything up to date.");
    }

    Ok(())
}

/// Result of checking and optionally fixing a single entry.
enum FixResult {
    /// Entry was already fine.
    Ok,
    /// Entry was fixed.
    Fixed,
    /// Entry was skipped.
    Skipped,
}

/// Checks a single entry and fixes it if needed.
fn check_and_fix(
    printer: &Printer,
    linker: &Linker,
    entry: &crate::config::LinkEntry,
    dest_path: &std::path::Path,
    force: bool,
    dry_run: bool,
) -> Result<FixResult> {
    let status = linker.check_entry(entry)?;
    match status {
        EntryStatus::Ok => Ok(FixResult::Ok),
        EntryStatus::Modified if !force => {
            printer.skipped("modified", dest_path);
            printer.hint("  use --force to overwrite");
            Ok(FixResult::Skipped)
        }
        EntryStatus::Missing | EntryStatus::BrokenSymlink | EntryStatus::Modified => {
            if dry_run {
                printer.action("would fix", dest_path);
                return Ok(FixResult::Skipped);
            }
            if dest_path.is_symlink() {
                let _ = std::fs::remove_file(dest_path);
            }
            match linker.deploy_entry(entry, force) {
                Result::Ok(DeployResult::Created) => {
                    printer.ok("fixed", dest_path);
                    Ok(FixResult::Fixed)
                }
                Result::Ok(_) => Ok(FixResult::Ok),
                Err(e) => {
                    printer.warn("error", &format!("{}: {e}", dest_path.display()));
                    Ok(FixResult::Skipped)
                }
            }
        }
        EntryStatus::Conflict => {
            printer.skipped("conflict", dest_path);
            Ok(FixResult::Skipped)
        }
    }
}
