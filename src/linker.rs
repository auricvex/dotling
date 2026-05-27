/// Symlink and copy deployment logic.
///
/// The [`Linker`] handles creating, removing, and checking the status of
/// deployed dotfile entries (symlinks or copies) between the repo and their
/// destination paths.
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

use crate::{
    config::{LinkEntry, LinkMethod},
    error::{DotlingError, Result, io_err},
    repo,
};

/// Result of a deploy operation.
#[derive(Debug, PartialEq, Eq)]
pub enum DeployResult {
    /// The artifact was newly created.
    Created,
    /// The correct artifact already existed — no action taken.
    AlreadyOk,
    /// The entry was skipped (e.g. copy differs, no `--force`).
    Skipped,
}

/// Status of a deployed entry.
#[derive(Debug, PartialEq, Eq)]
pub enum EntryStatus {
    /// The deployment is correct.
    Ok,
    /// The deployed copy differs from the repo source.
    Modified,
    /// The destination symlink is broken.
    BrokenSymlink,
    /// The destination does not exist.
    Missing,
    /// An unmanaged regular file exists at the destination.
    Conflict,
}

/// Core deploy/undeploy logic, holding the repo root path.
pub struct Linker {
    /// The absolute path to the dotling repository root.
    repo_root: PathBuf,
}

impl Linker {
    /// Creates a new linker for the given repo root.
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    /// Returns the absolute source path for an entry within the repo.
    fn abs_src(&self, entry: &LinkEntry) -> PathBuf {
        self.repo_root.join(&entry.src)
    }

    /// Returns the absolute destination path for an entry.
    fn abs_dest(entry: &LinkEntry) -> Result<PathBuf> {
        repo::src_to_dest_path(&entry.dest)
    }

    /// Deploys a single config entry as a symlink or copy.
    ///
    /// Idempotent: if the correct artifact already exists, returns
    /// [`DeployResult::AlreadyOk`]. For symlinks, returns
    /// [`DotlingError::DestinationConflict`] if an unmanaged file exists.
    /// For copies, returns [`DeployResult::Skipped`] if the content differs
    /// (use `--force` to overwrite).
    pub fn deploy_entry(&self, entry: &LinkEntry, force: bool) -> Result<DeployResult> {
        let src = self.abs_src(entry);
        let dest = Self::abs_dest(entry)?;

        // Ensure parent directories exist
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }

        match entry.method {
            LinkMethod::Symlink => deploy_symlink(&src, &dest),
            LinkMethod::Copy => deploy_copy(&src, &dest, force),
        }
    }

    /// Undeploys an entry — removes the symlink and copies the repo file
    /// back to the destination. For copies, does nothing (dest is already
    /// the real file).
    pub fn undeploy_entry(&self, entry: &LinkEntry) -> Result<()> {
        let src = self.abs_src(entry);
        let dest = Self::abs_dest(entry)?;

        match entry.method {
            LinkMethod::Symlink => {
                if dest.is_symlink() {
                    fs::remove_file(&dest).map_err(io_err(&dest))?;
                }
                // Copy the repo file back to dest
                if src.exists() {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent).map_err(io_err(parent))?;
                    }
                    fs::copy(&src, &dest).map_err(io_err(&dest))?;
                }
            }
            LinkMethod::Copy => {
                // For copies, the dest is already the real file — nothing to
                // do.
            }
        }
        Ok(())
    }

    /// Checks the deployment status of a single entry.
    pub fn check_entry(&self, entry: &LinkEntry) -> Result<EntryStatus> {
        let src = self.abs_src(entry);
        let dest = Self::abs_dest(entry)?;

        match entry.method {
            LinkMethod::Symlink => check_symlink(&src, &dest),
            LinkMethod::Copy => check_copy(&src, &dest),
        }
    }
}

/// Deploys a symlink from dest → src.
fn deploy_symlink(src: &Path, dest: &Path) -> Result<DeployResult> {
    if dest.is_symlink() {
        let target = fs::read_link(dest).map_err(io_err(dest))?;
        if target == src {
            return Ok(DeployResult::AlreadyOk);
        }
        // Different symlink — conflict
        return Err(DotlingError::DestinationConflict(dest.to_path_buf()));
    }
    if dest.exists() {
        // Regular file at dest — conflict
        return Err(DotlingError::DestinationConflict(dest.to_path_buf()));
    }
    symlink(src, dest).map_err(io_err(dest))?;
    Ok(DeployResult::Created)
}

/// Deploys a copy from src to dest.
fn deploy_copy(src: &Path, dest: &Path, force: bool) -> Result<DeployResult> {
    if dest.exists() && !dest.is_symlink() {
        let src_content = fs::read(src).map_err(io_err(src))?;
        let dest_content = fs::read(dest).map_err(io_err(dest))?;
        if src_content == dest_content {
            return Ok(DeployResult::AlreadyOk);
        }
        if !force {
            return Ok(DeployResult::Skipped);
        }
    } else if dest.is_symlink() {
        return Err(DotlingError::DestinationConflict(dest.to_path_buf()));
    }
    fs::copy(src, dest).map_err(io_err(dest))?;
    Ok(DeployResult::Created)
}

/// Checks a symlink entry's status.
fn check_symlink(src: &Path, dest: &Path) -> Result<EntryStatus> {
    if !dest.is_symlink() {
        if dest.exists() {
            return Ok(EntryStatus::Conflict);
        }
        return Ok(EntryStatus::Missing);
    }
    let target = fs::read_link(dest).map_err(io_err(dest))?;
    if target == src {
        // Check if the target actually exists
        if src.exists() {
            return Ok(EntryStatus::Ok);
        }
        return Ok(EntryStatus::BrokenSymlink);
    }
    Ok(EntryStatus::Conflict)
}

/// Checks a copy entry's status.
fn check_copy(src: &Path, dest: &Path) -> Result<EntryStatus> {
    if !dest.exists() {
        return Ok(EntryStatus::Missing);
    }
    if dest.is_symlink() {
        return Ok(EntryStatus::Conflict);
    }
    let src_content = fs::read(src).map_err(io_err(src))?;
    let dest_content = fs::read(dest).map_err(io_err(dest))?;
    if src_content == dest_content {
        Ok(EntryStatus::Ok)
    } else {
        Ok(EntryStatus::Modified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    fn make_entry(src: &str, dest: &str, method: LinkMethod) -> LinkEntry {
        LinkEntry {
            src: src.to_string(),
            dest: dest.to_string(),
            method,
            os: Platform::default(),
        }
    }

    #[test]
    fn deploy_symlink_creates_link() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let src_file = repo_root.join("shell/zshrc");
        fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        fs::write(&src_file, "# zshrc").unwrap();

        let dest_file = dir.path().join("home/.zshrc");
        let entry = make_entry(
            "shell/zshrc",
            &dest_file.to_string_lossy(),
            LinkMethod::Symlink,
        );

        let linker = Linker::new(repo_root);
        let result = linker.deploy_entry(&entry, false).unwrap();
        assert_eq!(result, DeployResult::Created);
        assert!(dest_file.is_symlink());
    }

    #[test]
    fn deploy_symlink_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let src_file = repo_root.join("shell/zshrc");
        fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        fs::write(&src_file, "# zshrc").unwrap();

        let dest_file = dir.path().join("home/.zshrc");
        let entry = make_entry(
            "shell/zshrc",
            &dest_file.to_string_lossy(),
            LinkMethod::Symlink,
        );

        let linker = Linker::new(repo_root);
        linker.deploy_entry(&entry, false).unwrap();
        let result = linker.deploy_entry(&entry, false).unwrap();
        assert_eq!(result, DeployResult::AlreadyOk);
    }

    #[test]
    fn deploy_symlink_conflict_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let src_file = repo_root.join("shell/zshrc");
        fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        fs::write(&src_file, "# zshrc").unwrap();

        let dest_file = dir.path().join("home/.zshrc");
        fs::create_dir_all(dest_file.parent().unwrap()).unwrap();
        fs::write(&dest_file, "existing content").unwrap();

        let entry = make_entry(
            "shell/zshrc",
            &dest_file.to_string_lossy(),
            LinkMethod::Symlink,
        );

        let linker = Linker::new(repo_root);
        let result = linker.deploy_entry(&entry, false);
        assert!(result.is_err());
    }

    #[test]
    fn check_entry_ok() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let src_file = repo_root.join("shell/zshrc");
        fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        fs::write(&src_file, "# zshrc").unwrap();

        let dest_file = dir.path().join("home/.zshrc");
        let entry = make_entry(
            "shell/zshrc",
            &dest_file.to_string_lossy(),
            LinkMethod::Symlink,
        );

        let linker = Linker::new(repo_root);
        linker.deploy_entry(&entry, false).unwrap();
        let status = linker.check_entry(&entry).unwrap();
        assert_eq!(status, EntryStatus::Ok);
    }

    #[test]
    fn check_entry_missing() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let src_file = repo_root.join("shell/zshrc");
        fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        fs::write(&src_file, "# zshrc").unwrap();

        let dest_file = dir.path().join("home/.zshrc");
        let entry = make_entry(
            "shell/zshrc",
            &dest_file.to_string_lossy(),
            LinkMethod::Symlink,
        );

        let linker = Linker::new(repo_root);
        let status = linker.check_entry(&entry).unwrap();
        assert_eq!(status, EntryStatus::Missing);
    }

    #[test]
    fn check_entry_broken_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let src_file = repo_root.join("shell/zshrc");
        fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        fs::write(&src_file, "# zshrc").unwrap();

        let dest_file = dir.path().join("home/.zshrc");
        let entry = make_entry(
            "shell/zshrc",
            &dest_file.to_string_lossy(),
            LinkMethod::Symlink,
        );

        let linker = Linker::new(repo_root.clone());
        linker.deploy_entry(&entry, false).unwrap();

        // Remove source to break the symlink
        fs::remove_file(&src_file).unwrap();
        let status = linker.check_entry(&entry).unwrap();
        assert_eq!(status, EntryStatus::BrokenSymlink);
    }
}
