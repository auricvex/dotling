/// Repository discovery and path utilities.
///
/// Manages the dotling repo root path stored at `~/.config/dotling/repo`
/// and provides path conversion utilities for mapping between home-directory
/// destinations and repo-relative source paths.
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{DotlingError, Result, io_err};

/// Relative path from home to the dotling discovery file.
const DISCOVERY_DIR: &str = ".config/dotling";

/// Filename of the discovery file.
const DISCOVERY_FILE: &str = "repo";

/// Returns the absolute path to the discovery file.
fn discovery_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or(DotlingError::RepoNotFound)?;
    Ok(home.join(DISCOVERY_DIR).join(DISCOVERY_FILE))
}

/// Reads the repo root path from the discovery file.
///
/// Returns [`DotlingError::RepoNotFound`] if the file does not exist.
pub fn get_repo_root() -> Result<PathBuf> {
    let path = discovery_path()?;
    if !path.exists() {
        return Err(DotlingError::RepoNotFound);
    }
    let content = fs::read_to_string(&path).map_err(io_err(&path))?;
    let root = PathBuf::from(content.trim_end_matches('\n'));
    if !root.exists() {
        return Err(DotlingError::RepoNotFound);
    }
    Ok(root)
}

/// Writes the repo root path to the discovery file.
///
/// The path is resolved to an absolute, normalized form before storage.
pub fn set_repo_root(repo_root: &Path) -> Result<()> {
    let resolved = resolve_path(repo_root)?;
    let path = discovery_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    let content = resolved.to_string_lossy();
    fs::write(&path, content.as_ref()).map_err(io_err(&path))?;
    Ok(())
}

/// Expands `~` at the start of a path to the user's home directory.
///
/// Returns the path unchanged if it is already absolute.
pub fn expand_path(path: &Path) -> Result<PathBuf> {
    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        let home = dirs::home_dir().ok_or(DotlingError::RepoNotFound)?;
        Ok(home.join(s.strip_prefix("~/").unwrap_or("")))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Resolves a user-supplied path to an absolute, normalized form.
///
/// 1. Expands `~` to the home directory.
/// 2. Prepends `std::env::current_dir()` if the path is still relative.
/// 3. Normalizes away `.` and `..` components **without** calling `fs::canonicalize`, so the result
///    never embeds machine-specific prefixes (e.g. `/Users/<username>`) that would break
///    portability.
pub fn resolve_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_path(path)?;
    let absolute = if expanded.is_relative() {
        std::env::current_dir()
            .map_err(|e| DotlingError::Io {
                path: expanded.clone(),
                source: e,
            })?
            .join(&expanded)
    } else {
        expanded
    };
    Ok(normalize_path(&absolute))
}

/// Normalizes `.` and `..` components out of an absolute path.
///
/// Unlike [`std::fs::canonicalize`], this does **not** resolve symlinks or
/// require the path to exist, keeping stored paths portable across machines.
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<std::path::Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {} // skip `.`
            std::path::Component::ParentDir => {
                // Pop the last normal component (but never pop past root)
                if parts
                    .last()
                    .is_some_and(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    parts.pop();
                }
            }
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

/// Checks whether a path is inside the user's home directory.
pub fn is_inside_home(path: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    path.starts_with(&home)
}

/// Bare dotfile name groupings for top-level dotfiles without a directory
/// component (e.g. `~/.zshrc` → `shell/zshrc`).
const SHELL_FILES: &[&str] = &[
    "zshrc",
    "bashrc",
    "bash_profile",
    "bash_aliases",
    "profile",
    "fishrc",
];
/// Git-related config files.
const GIT_FILES: &[&str] = &["gitconfig", "gitignore_global", "gitignore"];
/// Vim-related config files.
const VIM_FILES: &[&str] = &["vimrc", "ideavimrc"];
/// Tmux-related config files.
const TMUX_FILES: &[&str] = &["tmux.conf"];

/// Returns the group directory for a bare dotfile name.
fn group_for_bare_file(name: &str) -> &'static str {
    if SHELL_FILES.contains(&name) {
        "shell"
    } else if GIT_FILES.contains(&name) {
        "git"
    } else if VIM_FILES.contains(&name) {
        "vim"
    } else if TMUX_FILES.contains(&name) {
        "tmux"
    } else {
        "home"
    }
}

/// Converts an absolute destination path into a repo-relative source path.
///
/// - Strips the home directory prefix.
/// - Strips the leading dot from the first path component (`.config` → `config`).
/// - Bare top-level dotfiles are placed under a named group directory (`~/.zshrc` → `shell/zshrc`).
///
/// All returned paths use forward slashes.
pub fn dest_to_src_path(dest: &Path) -> Result<String> {
    let home = dirs::home_dir().ok_or(DotlingError::RepoNotFound)?;
    let relative = dest
        .strip_prefix(&home)
        .map_err(|_| DotlingError::PathOutsideHome(dest.to_path_buf()))?;

    let components: Vec<&str> = relative
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();

    if components.is_empty() {
        return Err(DotlingError::PathNotFound(dest.to_path_buf()));
    }

    // Single component = bare dotfile (e.g. `.zshrc`)
    if components.len() == 1 {
        let name = components[0].strip_prefix('.').unwrap_or(components[0]);
        let group = group_for_bare_file(name);
        return Ok(format!("{group}/{name}"));
    }

    // Multi-component: strip leading dot from first component
    let mut parts: Vec<String> = components.iter().map(|s| (*s).to_string()).collect();
    if let Some(first) = parts.first_mut()
        && let Some(stripped) = first.strip_prefix('.')
    {
        *first = stripped.to_string();
    }

    Ok(parts.join("/"))
}

/// Converts a repo-relative source path back to an absolute destination path
/// using the stored dest string from the config entry.
///
/// This is the reverse of [`dest_to_src_path`], but since the mapping is
/// not perfectly reversible without the original dest, we take the dest
/// string directly.
pub fn src_to_dest_path(dest_str: &str) -> Result<PathBuf> {
    let path = Path::new(dest_str);
    expand_path(path)
}

/// Converts an absolute path to use `~` prefix for the home directory.
///
/// If the path is inside the home directory, the home prefix is replaced
/// with `~`. Otherwise the path is returned unchanged.
pub fn path_with_tilde(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rel) = path.strip_prefix(&home)
    {
        return format!("~/{}", rel.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion() {
        let expanded = expand_path(Path::new("~/something")).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(expanded, home.join("something"));
    }

    #[test]
    fn absolute_path_passthrough() {
        let path = Path::new("/usr/local/bin/thing");
        let result = expand_path(path).unwrap();
        assert_eq!(result, PathBuf::from("/usr/local/bin/thing"));
    }

    #[test]
    fn dest_to_src_config_nvim() {
        let home = dirs::home_dir().unwrap();
        let dest = home.join(".config/nvim/init.lua");
        let src = dest_to_src_path(&dest).unwrap();
        assert_eq!(src, "config/nvim/init.lua");
    }

    #[test]
    fn dest_to_src_bare_zshrc() {
        let home = dirs::home_dir().unwrap();
        let dest = home.join(".zshrc");
        let src = dest_to_src_path(&dest).unwrap();
        assert_eq!(src, "shell/zshrc");
    }

    #[test]
    fn dest_to_src_bare_gitconfig() {
        let home = dirs::home_dir().unwrap();
        let dest = home.join(".gitconfig");
        let src = dest_to_src_path(&dest).unwrap();
        assert_eq!(src, "git/gitconfig");
    }

    #[test]
    fn is_inside_home_true() {
        let home = dirs::home_dir().unwrap();
        assert!(is_inside_home(&home.join("test")));
    }

    #[test]
    fn is_inside_home_false() {
        assert!(!is_inside_home(Path::new("/tmp/test")));
    }

    #[test]
    fn resolve_tilde_path() {
        let resolved = resolve_path(Path::new("~/something")).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolved, home.join("something"));
    }

    #[test]
    fn resolve_absolute_path() {
        let resolved = resolve_path(Path::new("/usr/local/bin")).unwrap();
        assert_eq!(resolved, PathBuf::from("/usr/local/bin"));
    }

    #[test]
    fn normalize_removes_dot_dot() {
        let path = PathBuf::from("/home/user/foo/../bar");
        assert_eq!(normalize_path(&path), PathBuf::from("/home/user/bar"));
    }

    #[test]
    fn normalize_removes_dot() {
        let path = PathBuf::from("/home/user/./bar");
        assert_eq!(normalize_path(&path), PathBuf::from("/home/user/bar"));
    }

    #[test]
    fn normalize_complex_path() {
        let path = PathBuf::from("/a/b/../c/./d/../e");
        assert_eq!(normalize_path(&path), PathBuf::from("/a/c/e"));
    }
}
