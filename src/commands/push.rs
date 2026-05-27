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
