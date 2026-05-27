/// Push changes to the remote repository.
///
/// Stages all changes, commits with the provided message (or a default),
/// and pushes to the remote. Requires a configured remote.
use crate::{
    error::{DotlingError, Result},
    git::Git,
    printer::Printer,
    repo,
};

/// Default commit message.
const DEFAULT_MESSAGE: &str = "dotling: update dotfiles";

/// Runs the `push` command.
pub fn run(printer: &Printer, message: Option<&str>) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let git = Git::new(repo_root.clone());

    if !git.has_remote()? {
        return Err(DotlingError::NoRemote);
    }

    // Auto pull-back modified entries before pushing
    let config = crate::config::Config::load(&repo_root)?;
    let linker = crate::linker::Linker::new(repo_root.clone());
    let mut pulled_back = 0;

    for entry in config.active_entries() {
        if linker.check_entry(entry)? == crate::linker::EntryStatus::Modified {
            let dest_path = repo::src_to_dest_path(&entry.dest)?;
            let src_path = repo_root.join(&entry.src);
            
            match entry.method {
                crate::config::LinkMethod::Copy => {
                    printer.arrow("pull-back", &dest_path, &src_path);
                    std::fs::copy(&dest_path, &src_path).map_err(crate::error::io_err(&src_path))?;
                    pulled_back += 1;
                }
                crate::config::LinkMethod::Encrypted => {
                    printer.arrow("encrypt pull", &dest_path, &src_path);
                    let plaintext = std::fs::read(&dest_path).map_err(crate::error::io_err(&dest_path))?;
                    let ciphertext = crate::crypto::encrypt(&plaintext, &config.encryption.recipients)?;
                    std::fs::write(&src_path, ciphertext).map_err(crate::error::io_err(&src_path))?;
                    pulled_back += 1;
                }
                crate::config::LinkMethod::Symlink => {}
            }
        }
    }
    
    if pulled_back > 0 {
        printer.annotation(&format!("Automatically pulled back {} modified entry/entries.", pulled_back));
    }

    let msg = message.unwrap_or(DEFAULT_MESSAGE);

    printer.action("stage", &repo_root);
    git.stage_all()?;

    printer.action("commit", &repo_root);
    git.commit(msg)?;

    printer.action("push", &repo_root);
    git.push()?;

    printer.success("Pushed successfully.");
    Ok(())
}
