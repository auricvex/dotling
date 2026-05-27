use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Recursively walk a directory, returning all file paths.
///
/// Skips hidden files/directories (starting with `.`) unless `include_hidden`
/// is `true`.
pub fn walk_dir(root: &Path, include_hidden: bool) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    walk_dir_inner(root, include_hidden, &mut results)?;
    results.sort();
    Ok(results)
}

fn walk_dir_inner(
    dir: &Path,
    include_hidden: bool,
    results: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden unless requested
        if !include_hidden && name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            walk_dir_inner(&path, include_hidden, results)?;
        } else {
            results.push(path);
        }
    }

    Ok(())
}

/// Copy a file atomically: write to a temp file in the same directory, then rename.
pub fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
    }

    // Write to temp then rename (atomic on same filesystem)
    let tmp = dst.with_extension("dotling-tmp");
    let content = fs::read(src).map_err(|e| Error::io(src, "read", e))?;

    let mut file = fs::File::create(&tmp).map_err(|e| Error::io(&tmp, "create temp file", e))?;
    file.write_all(&content)
        .map_err(|e| Error::io(&tmp, "write temp file", e))?;
    file.sync_all()
        .map_err(|e| Error::io(&tmp, "sync temp file", e))?;
    drop(file);

    fs::rename(&tmp, dst).map_err(|e| Error::io(dst, "rename temp file", e))?;
    Ok(())
}

/// Create a symlink from `link` pointing to `target`.
///
/// Creates parent directories as needed.
pub fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
            .map_err(|e| Error::io(link, "create symlink", e))?;
    }

    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
                .map_err(|e| Error::io(link, "create symlink", e))?;
        } else {
            std::os::windows::fs::symlink_file(target, link)
                .map_err(|e| Error::io(link, "create symlink", e))?;
        }
    }

    Ok(())
}

/// Remove a symlink without touching its target.
pub fn remove_symlink(path: &Path) -> Result<()> {
    // On Unix, symlinks are removed with `remove_file` regardless of target type.
    // On Windows, we need to distinguish.
    #[cfg(unix)]
    {
        fs::remove_file(path).map_err(|e| Error::io(path, "remove symlink", e))?;
    }

    #[cfg(windows)]
    {
        let meta = fs::symlink_metadata(path).map_err(|e| Error::io(path, "read metadata", e))?;
        if meta.is_dir() {
            fs::remove_dir(path).map_err(|e| Error::io(path, "remove symlink", e))?;
        } else {
            fs::remove_file(path).map_err(|e| Error::io(path, "remove symlink", e))?;
        }
    }

    Ok(())
}

/// Write data to a file atomically.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create directory", e))?;
    }

    let tmp = path.with_extension("dotling-tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| Error::io(&tmp, "create temp file", e))?;
    file.write_all(data)
        .map_err(|e| Error::io(&tmp, "write temp file", e))?;
    file.sync_all()
        .map_err(|e| Error::io(&tmp, "sync temp file", e))?;
    drop(file);

    fs::rename(&tmp, path).map_err(|e| Error::io(path, "rename temp file", e))?;
    Ok(())
}

/// Check if a path is a symlink.
pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_symlink())
}

/// Read the target of a symlink.
pub fn read_link(path: &Path) -> Result<PathBuf> {
    fs::read_link(path).map_err(|e| Error::io(path, "read symlink target", e))
}

/// Check if two files have identical contents.
pub fn files_identical(a: &Path, b: &Path) -> Result<bool> {
    let content_a = fs::read(a).map_err(|e| Error::io(a, "read", e))?;
    let content_b = fs::read(b).map_err(|e| Error::io(b, "read", e))?;
    Ok(content_a == content_b)
}

/// Remove empty parent directories up to (but not including) `stop_at`.
pub fn cleanup_empty_parents(path: &Path, stop_at: &Path) -> Result<()> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == stop_at || dir.components().count() <= 1 {
            break;
        }
        // Try to remove — will fail if not empty, which is fine
        if fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_finds_files() {
        let dir = std::env::temp_dir().join("dotling_test_walk");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/file1.txt"), "hello").unwrap();
        fs::write(dir.join("a/b/file2.txt"), "world").unwrap();
        fs::write(dir.join("a/.hidden"), "secret").unwrap();

        let files = walk_dir(&dir, false).unwrap();
        assert_eq!(files.len(), 2);

        let files_hidden = walk_dir(&dir, true).unwrap();
        assert_eq!(files_hidden.len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_roundtrip() {
        let path = std::env::temp_dir().join("dotling_test_atomic");
        atomic_write(&path, b"test data").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "test data");
        let _ = fs::remove_file(&path);
    }
}
