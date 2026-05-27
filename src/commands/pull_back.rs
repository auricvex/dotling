/// Pull back a deployed copy into the repo.
///
/// For copied files: copies the deployed file back to the repo source and
/// stages it. For symlinked files: explains the file IS the source and
/// suggests `dotling push`. Resolves by filename or full dest path; errors
/// if ambiguous.
use std::{fs, path::Path};

use crate::{
    config::{Config, LinkMethod},
    error::{DotlingError, Result, io_err},
    git::Git,
    printer::Printer,
    repo,
};

/// Runs the `pull-back` command.
pub fn run(printer: &Printer, file: &str) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let config = Config::load(&repo_root)?;
    let git = Git::new(repo_root.clone());

    // Try to find the entry by full dest path or by filename
    let entry = find_entry(&config, file)?;

    match entry.method {
        LinkMethod::Symlink => {
            let dest_path = repo::src_to_dest_path(&entry.dest)?;
            printer.annotation(&format!(
                "\"{}\" is deployed as a symlink — it already IS the repo source.",
                dest_path.display()
            ));
            printer.hint("Use `dotling push` to commit and push changes.");
            Ok(())
        }
        LinkMethod::Copy => {
            let dest_path = repo::src_to_dest_path(&entry.dest)?;
            let src_path = repo_root.join(&entry.src);

            if !dest_path.exists() {
                return Err(DotlingError::PathNotFound(dest_path));
            }

            printer.arrow("pull-back", &dest_path, &src_path);
            fs::copy(&dest_path, &src_path).map_err(io_err(&src_path))?;

            git.stage(&src_path)?;
            printer.ok("staged", &src_path);
            printer.hint("Use `dotling push` to commit and push.");

            Ok(())
        }
        LinkMethod::Encrypted => {
            let dest_path = repo::src_to_dest_path(&entry.dest)?;
            let src_path = repo_root.join(&entry.src);

            if !dest_path.exists() {
                return Err(DotlingError::PathNotFound(dest_path));
            }

            printer.arrow("encrypt pull", &dest_path, &src_path);
            let plaintext = fs::read(&dest_path).map_err(io_err(&dest_path))?;
            let ciphertext = crate::crypto::encrypt(&plaintext, &config.encryption.recipients)?;
            fs::write(&src_path, ciphertext).map_err(io_err(&src_path))?;

            git.stage(&src_path)?;
            printer.ok("staged", &src_path);
            printer.hint("Use `dotling push` to commit and push.");

            Ok(())
        }
    }
}

/// Finds an entry by full dest path or filename, erroring on ambiguity.
fn find_entry<'a>(config: &'a Config, file: &str) -> Result<&'a crate::config::LinkEntry> {
    // First try exact dest match
    let expanded = repo::resolve_path(Path::new(file))?;
    let dest_str = repo::path_with_tilde(&expanded);

    if let Some(entry) = config.find_by_dest(&dest_str) {
        return Ok(entry);
    }

    // Try matching by filename
    let filename = Path::new(file)
        .file_name()
        .map_or_else(|| file.to_string(), |f| f.to_string_lossy().to_string());

    let matches: Vec<_> = config
        .entries
        .iter()
        .filter(|e| {
            Path::new(&e.dest)
                .file_name()
                .is_some_and(|f| f.to_string_lossy() == filename)
                || Path::new(&e.src)
                    .file_name()
                    .is_some_and(|f| f.to_string_lossy() == filename)
        })
        .collect();

    match matches.len() {
        0 => Err(DotlingError::NotTracked(expanded)),
        1 => Ok(matches[0]),
        _ => {
            let paths: Vec<_> = matches.iter().map(|e| e.dest.as_str()).collect();
            Err(DotlingError::NotTracked(std::path::PathBuf::from(format!(
                "ambiguous filename '{}' — matches: {}. Use the full dest path.",
                filename,
                paths.join(", ")
            ))))
        }
    }
}
