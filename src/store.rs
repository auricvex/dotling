use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::path;

const STATE_FILE: &str = "state.toml";

/// Global state directory: `~/.dotling/`
pub fn state_dir() -> Result<PathBuf> {
    Ok(path::home_dir()?.join(".dotling"))
}

/// Path to the global state file: `~/.dotling/state.toml`
fn state_path() -> Result<PathBuf> {
    Ok(state_dir()?.join(STATE_FILE))
}

/// Get the currently registered repo root.
///
/// Returns `None` if no repo has been initialized.
pub fn get_repo_root() -> Result<Option<PathBuf>> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| Error::io(&path, "read state", e))?;

    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "repo" {
                let value = value.trim().trim_matches('"');
                let expanded = path::expand_tilde(Path::new(value))?;
                return Ok(Some(expanded));
            }
        }
    }

    Ok(None)
}

/// Register a repo root in the global state.
pub fn set_repo_root(repo_root: &Path) -> Result<()> {
    let dir = state_dir()?;
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, "create state directory", e))?;

    let display_path = path::collapse_tilde(repo_root);
    let content = format!(
        "# dotling global state — managed by dotling\nrepo = \"{}\"\n",
        display_path.display()
    );

    crate::fs::atomic_write(&state_path()?, content.as_bytes())
}

/// Require a repo root to be configured. Returns an error with a helpful
/// message if no repo is registered.
pub fn require_repo_root() -> Result<PathBuf> {
    get_repo_root()?.ok_or_else(|| {
        Error::User(
            "no dotfiles repository found — run `dotling init <path>` first".into(),
        )
    })
}

/// Path to `dotling.toml` within a repo.
pub fn config_path(repo_root: &Path) -> PathBuf {
    repo_root.join("dotling.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_is_under_home() {
        let dir = state_dir().unwrap();
        assert!(dir.ends_with(".dotling"));
    }
}
