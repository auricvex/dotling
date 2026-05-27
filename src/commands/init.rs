/// Initialize a new dotling repository, adopt an existing one, or clone
/// a remote.
///
/// Detection order for a local path:
/// 1. `.git/` + `.dotling.toml` → **adopt**: set repo root, deploy all entries.
/// 2. `.git/` only → **adopt git repo**: set repo root, create empty config.
/// 3. Neither → **fresh init**: create directory, `git init`, empty config.
///
/// If the argument looks like a git URL → clone to `~/dotfiles`, set repo
/// root, load config, deploy all entries.
use std::{fs, path::PathBuf};

use crate::{
    config::Config,
    error::{DotlingError, Result, io_err},
    git::Git,
    linker::Linker,
    printer::Printer,
    repo,
};

/// Returns `true` if the argument looks like a git URL.
fn is_git_url(s: &str) -> bool {
    s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("ssh://")
}

/// Runs the `init` command.
pub fn run(printer: &Printer, path_or_url: &str) -> Result<()> {
    // Check if already initialized
    if repo::get_repo_root().is_ok() {
        let root = repo::get_repo_root()?;
        return Err(DotlingError::AlreadyInitialized(root));
    }

    if is_git_url(path_or_url) {
        clone_init(printer, path_or_url)
    } else {
        local_init(printer, path_or_url)
    }
}

/// Initializes by cloning a remote repository.
fn clone_init(printer: &Printer, url: &str) -> Result<()> {
    let home = dirs::home_dir().ok_or(DotlingError::RepoNotFound)?;
    let dest = home.join("dotfiles");

    if dest.exists() {
        return Err(DotlingError::AlreadyInitialized(dest));
    }

    printer.action("clone", &dest);
    Git::clone(url, &dest)?;

    repo::set_repo_root(&dest)?;

    deploy_config_entries(printer, &dest)?;

    printer.success("Repository cloned and entries deployed.");
    Ok(())
}

/// Initializes a new local repository, or adopts an existing one.
fn local_init(printer: &Printer, path: &str) -> Result<()> {
    let repo_path = repo::resolve_path(&PathBuf::from(path))?;

    let has_git = repo_path.join(".git").exists();
    let has_config = repo_path.join(crate::config::CONFIG_FILE).exists();

    if has_git && has_config {
        adopt_existing(printer, &repo_path)
    } else if has_git {
        adopt_git_repo(printer, &repo_path)
    } else {
        fresh_init(printer, &repo_path)
    }
}

/// Adopts an existing repository that already has a `.dotling.toml`.
///
/// Sets the repo root and deploys all config entries.
fn adopt_existing(printer: &Printer, repo_path: &std::path::Path) -> Result<()> {
    printer.action("adopt", repo_path);

    repo::set_repo_root(repo_path)?;
    deploy_config_entries(printer, repo_path)?;

    printer.success("Adopted existing dotfiles repository.");
    Ok(())
}

/// Adopts an existing git repository that has no `.dotling.toml` yet.
///
/// Sets the repo root and writes an empty config.
fn adopt_git_repo(printer: &Printer, repo_path: &std::path::Path) -> Result<()> {
    printer.action("adopt", repo_path);

    let config = Config::load(repo_path)?;
    config.save()?;

    repo::set_repo_root(repo_path)?;

    printer.success("Adopted git repository — no entries to deploy yet.");
    printer.hint("Use `dotling link <path>` to start tracking dotfiles.");
    Ok(())
}

/// Creates a brand-new dotling repository from scratch.
fn fresh_init(printer: &Printer, repo_path: &std::path::Path) -> Result<()> {
    if repo_path.join(crate::config::CONFIG_FILE).exists() {
        return Err(DotlingError::AlreadyInitialized(repo_path.to_path_buf()));
    }

    fs::create_dir_all(repo_path).map_err(io_err(repo_path))?;

    printer.action("init", repo_path);

    let git = Git::new(repo_path.to_path_buf());
    git.init()?;

    // Write empty config
    let config = Config::load(repo_path)?;
    config.save()?;

    repo::set_repo_root(repo_path)?;

    printer.success("Initialized empty dotling repository.");
    printer.hint(&format!(
        "Add a remote with: cd {} && git remote add origin <url>",
        repo_path.display()
    ));
    Ok(())
}

/// Loads the config and deploys all active entries, printing status for each.
fn deploy_config_entries(printer: &Printer, repo_path: &std::path::Path) -> Result<()> {
    let config = Config::load(repo_path)?;
    let linker = Linker::new(repo_path.to_path_buf());

    for entry in config.active_entries() {
        let dest_path = repo::src_to_dest_path(&entry.dest)?;
        match linker.deploy_entry(entry, false) {
            Ok(crate::linker::DeployResult::Created) => {
                printer.ok("linked", &dest_path);
            }
            Ok(crate::linker::DeployResult::AlreadyOk) => {
                printer.skipped("exists", &dest_path);
            }
            Ok(crate::linker::DeployResult::Skipped) => {
                printer.skipped("skipped", &dest_path);
            }
            Err(e) => {
                printer.warn("warn", &e.to_string());
            }
        }
    }

    Ok(())
}
