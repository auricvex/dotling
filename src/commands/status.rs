/// Show status of all tracked entries.
///
/// Checks each entry's deployment status and prints a summary
/// with hints for resolving issues.
use crate::{
    config::Config,
    error::Result,
    linker::{EntryStatus, Linker},
    printer::Printer,
    repo,
};

/// Runs the `status` command.
pub fn run(printer: &Printer) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let config = Config::load(&repo_root)?;
    let linker = Linker::new(repo_root);

    if config.entries.is_empty() {
        printer.annotation("No tracked entries. Use `dotling link <path>` to start tracking.");
        return Ok(());
    }

    let mut ok_count = 0usize;
    let mut modified_count = 0usize;
    let mut missing_count = 0usize;

    for entry in config.active_entries() {
        let dest_path = repo::src_to_dest_path(&entry.dest)?;
        match linker.check_entry(entry) {
            Ok(EntryStatus::Ok) => {
                printer.ok("ok", &dest_path);
                ok_count += 1;
            }
            Ok(EntryStatus::Modified) => {
                printer.skipped("modified", &dest_path);
                modified_count += 1;
            }
            Ok(EntryStatus::BrokenSymlink) => {
                printer.error_line("broken", &dest_path);
                missing_count += 1;
            }
            Ok(EntryStatus::Missing) => {
                printer.missing(&dest_path);
                missing_count += 1;
            }
            Ok(EntryStatus::Conflict) => {
                printer.error_line("conflict", &dest_path);
                missing_count += 1;
            }
            Err(e) => {
                printer.warn("error", &format!("{}: {e}", dest_path.display()));
                missing_count += 1;
            }
        }
    }

    printer.summary(ok_count, modified_count, missing_count);

    if modified_count > 0 {
        printer.hint("Use `dotling push` to push modifications, `dotling pull-back --all` to update the repo locally, or `dotling apply --force` to overwrite.");
    }
    if missing_count > 0 {
        printer.hint("Use `dotling apply` to re-deploy missing entries.");
    }

    Ok(())
}
