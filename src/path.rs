use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Returns the user's home directory.
///
/// Tries `$HOME` on Unix, `%USERPROFILE%` on Windows.
pub fn home_dir() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| Error::User("could not determine home directory ($HOME is unset)".into()))
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .ok_or_else(|| {
                Error::User(
                    "could not determine home directory (%USERPROFILE% is unset)".into(),
                )
            })
    }
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &Path) -> Result<PathBuf> {
    let s = path.to_string_lossy();
    if s == "~" {
        home_dir()
    } else if let Some(rest) = s.strip_prefix("~/") {
        Ok(home_dir()?.join(rest))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Replace a home directory prefix with `~`.
pub fn collapse_tilde(path: &Path) -> PathBuf {
    if let Ok(home) = home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return PathBuf::from("~").join(rest);
        }
    }
    path.to_path_buf()
}

/// Compute a relative path from `base` to `target`.
///
/// Returns `None` if no relative path can be computed (different prefixes
/// or non-absolute paths).
pub fn relative_to(target: &Path, base: &Path) -> Option<PathBuf> {
    // Both must be absolute
    if !target.is_absolute() || !base.is_absolute() {
        return None;
    }

    let mut target_parts = target.components().peekable();
    let mut base_parts = base.components().peekable();

    // Skip common prefix
    while let (Some(a), Some(b)) = (target_parts.peek(), base_parts.peek()) {
        if a != b {
            break;
        }
        target_parts.next();
        base_parts.next();
    }

    // Go up from base for each remaining component
    let mut result = PathBuf::new();
    for _ in base_parts {
        result.push("..");
    }

    // Append remaining target components
    for part in target_parts {
        result.push(part);
    }

    Some(result)
}

/// Category mapping rules for organizing dotfiles in the repo.
///
/// These patterns determine how home-directory paths map to repo subdirectories.
const CATEGORY_RULES: &[(&str, &[&str])] = &[
    ("shell", &[".zshrc", ".zshenv", ".zprofile", ".bashrc", ".bash_profile", ".profile", ".fishrc"]),
    ("git", &[".gitconfig", ".gitignore_global"]),
    ("vim", &[".vimrc", ".gvimrc"]),
    ("tmux", &[".tmux.conf"]),
    ("ssh", &[".ssh"]),
    ("gnupg", &[".gnupg"]),
];

/// Map a home-directory path to a repo-relative path.
///
/// Examples:
/// - `~/.config/nvim` → `config/nvim`
/// - `~/.zshrc` → `shell/zshrc`
/// - `~/.gitconfig` → `git/gitconfig`
/// - `~/.somerc` → `home/somerc`
pub fn map_to_repo(home_path: &Path) -> Result<PathBuf> {
    let home = home_dir()?;
    let rel = home_path
        .strip_prefix(&home)
        .map_err(|_| Error::User(format!("`{}` is not inside the home directory", home_path.display())))?;

    let rel_str = rel.to_string_lossy();

    // .config/* → config/*
    if let Some(rest) = rel_str.strip_prefix(".config/") {
        return Ok(PathBuf::from("config").join(rest));
    }
    if rel_str == ".config" {
        return Ok(PathBuf::from("config"));
    }

    // Check named category rules
    let file_name = rel
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default();

    for &(category, patterns) in CATEGORY_RULES {
        for &pattern in patterns {
            if rel_str == pattern.trim_start_matches('.') || file_name == pattern {
                let clean_name = file_name.trim_start_matches('.');
                return Ok(PathBuf::from(category).join(clean_name));
            }
        }
    }

    // Default: home/<name-without-dot>
    let clean = rel_str.trim_start_matches('.');
    Ok(PathBuf::from("home").join(clean))
}

/// Resolve a path to an absolute, canonicalized path.
/// Expands `~` and resolves `.` / `..` components.
pub fn resolve(path: &Path) -> Result<PathBuf> {
    let expanded = expand_tilde(path)?;
    if expanded.is_absolute() {
        Ok(normalize(&expanded))
    } else {
        let cwd = std::env::current_dir().map_err(|e| Error::io(".", "get current directory", e))?;
        Ok(normalize(&cwd.join(&expanded)))
    }
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion() {
        let expanded = expand_tilde(Path::new("~/test")).unwrap();
        assert!(expanded.is_absolute());
        assert!(expanded.ends_with("test"));
    }

    #[test]
    fn tilde_collapse() {
        let home = home_dir().unwrap();
        let path = home.join("Documents/file.txt");
        assert_eq!(collapse_tilde(&path), PathBuf::from("~/Documents/file.txt"));
    }

    #[test]
    fn relative_path() {
        let r = relative_to(Path::new("/a/b/c"), Path::new("/a/d")).unwrap();
        assert_eq!(r, PathBuf::from("../b/c"));
    }

    #[test]
    fn config_mapping() {
        let home = home_dir().unwrap();
        let p = home.join(".config/nvim/init.lua");
        assert_eq!(map_to_repo(&p).unwrap(), PathBuf::from("config/nvim/init.lua"));
    }

    #[test]
    fn shell_mapping() {
        let home = home_dir().unwrap();
        let p = home.join(".zshrc");
        assert_eq!(map_to_repo(&p).unwrap(), PathBuf::from("shell/zshrc"));
    }

    #[test]
    fn default_mapping() {
        let home = home_dir().unwrap();
        let p = home.join(".some_random_rc");
        assert_eq!(
            map_to_repo(&p).unwrap(),
            PathBuf::from("home/some_random_rc")
        );
    }

    #[test]
    fn normalize_dots() {
        let p = normalize(Path::new("/a/b/../c/./d"));
        assert_eq!(p, PathBuf::from("/a/c/d"));
    }
}
