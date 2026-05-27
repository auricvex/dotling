/// Unlink files from the dotling repository.
///
/// Undeploys the artifact, removes the entry from config. With `--purge`,
/// also deletes the source file from the repo. Always re-stages after.
use std::{fs, path::Path};

use crate::{
    config::Config,
    error::{Result, io_err},
    git::Git,
    linker::Linker,
    printer::Printer,
    repo,
};

/// Runs the `unlink` command.
pub fn run(printer: &Printer, path: &Path, purge: bool) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let abs_path = repo::resolve_path(path)?;
    let dest_str = repo::path_with_tilde(&abs_path);

    let mut config = Config::load(&repo_root)?;
    let linker = Linker::new(repo_root.clone());
    let git = Git::new(repo_root.clone());

    // Find and remove the entry
    let entry = config.remove_entry(&dest_str)?;

    // Undeploy
    linker.undeploy_entry(&entry)?;
    printer.ok("unlinked", &abs_path);

    // Purge if requested
    if purge {
        let src_abs = repo_root.join(&entry.src);
        if src_abs.exists() {
            fs::remove_file(&src_abs).map_err(io_err(&src_abs))?;
            printer.action("purged", &src_abs);

            // Clean up empty parent directories
            clean_empty_parents(&src_abs, &repo_root);
        }
    }

    config.save()?;
    git.stage_all()?;
    git.commit("dotling: unlink files")?;

    Ok(())
}

/// Removes empty parent directories up to (but not including) the repo root.
fn clean_empty_parents(path: &Path, repo_root: &Path) {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == repo_root {
            break;
        }
        if fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none()) {
            let _ = fs::remove_dir(dir);
        } else {
            break;
        }
        current = dir.parent();
    }
}
