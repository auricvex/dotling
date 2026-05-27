use std::{fs, path::Path, process::Command};

use crate::{
    config::Config,
    error::{Error, Result},
    store, ui,
};

/// Initialize a new dotfiles repository or adopt an existing one.
pub fn run(path_or_url: &str) -> Result<()> {
    // Check if it looks like a git URL
    if is_git_url(path_or_url) {
        return clone_repo(path_or_url);
    }

    let path = crate::path::expand_tilde(Path::new(path_or_url))?;
    let path = if path.is_absolute() {
        path
    } else {
        let cwd = std::env::current_dir().map_err(|e| Error::io(".", "get cwd", e))?;
        cwd.join(&path)
    };

    // Check if already initialized
    if let Ok(Some(existing)) = store::get_repo_root() {
        if existing == path {
            return Err(Error::User(format!(
                "already initialized at `{}`",
                path.display()
            )));
        }
    }

    let config_path = store::config_path(&path);

    if path.exists() {
        // Adopt existing directory
        if config_path.exists() {
            // Already has dotling.toml — adopt it
            let config = Config::load(&config_path)?;
            store::set_repo_root(&path)?;
            ui::success(&format!(
                "adopted existing repo at `{}` ({} entries)",
                path.display(),
                config.entries.len()
            ));
            return Ok(());
        }

        // Existing directory without dotling.toml — create config
        if !path.is_dir() {
            return Err(Error::User(format!(
                "`{}` exists and is not a directory",
                path.display()
            )));
        }

        let config = Config::new(config_path);
        config.save()?;
        store::set_repo_root(&path)?;

        // Initialize git if not already a git repo
        if !path.join(".git").exists() {
            init_git(&path)?;
        }

        ui::success(&format!("initialized at `{}`", path.display()));
        return Ok(());
    }

    // Create new directory
    fs::create_dir_all(&path).map_err(|e| Error::io(&path, "create directory", e))?;

    let config = Config::new(config_path);
    config.save()?;
    init_git(&path)?;
    store::set_repo_root(&path)?;

    ui::success(&format!("initialized new repo at `{}`", path.display()));
    ui::hint("add files with `dotling add <path>`");

    Ok(())
}

/// Clone a git URL into ~/dotfiles and register it.
fn clone_repo(url: &str) -> Result<()> {
    let home = crate::path::home_dir()?;
    let dest = home.join("dotfiles");

    if dest.exists() {
        return Err(Error::User(format!(
            "`{}` already exists — remove it first or use `dotling init {}`",
            dest.display(),
            dest.display()
        )));
    }

    ui::info(&format!("cloning `{url}`..."));

    let output = Command::new("git")
        .args(["clone", url, &dest.to_string_lossy()])
        .output()
        .map_err(|e| Error::io("git", "clone repository", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::User(format!("git clone failed: {}", stderr.trim())));
    }

    let config_path = store::config_path(&dest);
    if !config_path.exists() {
        let config = Config::new(config_path);
        config.save()?;
    }

    store::set_repo_root(&dest)?;

    let config = Config::load(&store::config_path(&dest))?;
    ui::success(&format!(
        "cloned to `{}` ({} entries)",
        dest.display(),
        config.entries.len()
    ));
    ui::hint("run `dotling deploy` to set up symlinks");

    Ok(())
}

/// Initialize a git repo at the given path.
fn init_git(path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .map_err(|e| Error::io("git", "init repository", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::User(format!("git init failed: {}", stderr.trim())));
    }

    Ok(())
}

/// Check if a string looks like a git URL.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn is_git_url(s: &str) -> bool {
    s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("ssh://")
        || s.starts_with("git://")
        || s.ends_with(".git")
}
