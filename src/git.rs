/// Git operations wrapper.
///
/// All git operations are executed via `std::process::Command::new("git")`
/// with `current_dir` set to the repo root. Never shells out to `sh`.
/// Non-zero exit codes produce [`DotlingError::Git`].
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::{DotlingError, Result};

/// Result of a `git pull --rebase` operation.
#[derive(Debug, PartialEq, Eq)]
pub enum PullResult {
    /// The repo was already up-to-date.
    UpToDate,
    /// The repo was updated, with this many files changed.
    Updated(usize),
    /// There was a merge/rebase conflict.
    Conflict,
}

/// Wrapper for git CLI operations on a repository.
pub struct Git {
    /// The absolute path to the repository root.
    repo_root: PathBuf,
}

impl Git {
    /// Creates a new git wrapper for the given repo root.
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    /// Runs a git command and returns its stdout on success.
    fn run(&self, args: &[&str]) -> Result<String> {
        run_git_at(&self.repo_root, args)
    }

    /// Initializes a new git repository.
    pub fn init(&self) -> Result<()> {
        self.run(&["init"])?;
        Ok(())
    }

    /// Adds a remote to the repository.
    #[allow(dead_code)]
    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        self.run(&["remote", "add", name, url])?;
        Ok(())
    }

    /// Stages a single file.
    pub fn stage(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        self.run(&["add", &path_str])?;
        Ok(())
    }

    /// Stages all changes in the repository.
    pub fn stage_all(&self) -> Result<()> {
        self.run(&["add", "-A"])?;
        Ok(())
    }

    /// Commits staged changes with the given message.
    ///
    /// No-op if there is nothing to commit.
    pub fn commit(&self, message: &str) -> Result<()> {
        // Check if there are staged changes
        let status = self.run(&["status", "--porcelain"]);
        match status {
            Ok(output) if output.trim().is_empty() => return Ok(()),
            Err(e) => return Err(e),
            _ => {}
        }
        self.run(&["commit", "-m", message])?;
        Ok(())
    }

    /// Pulls from the remote with `--rebase`.
    pub fn pull_rebase(&self) -> Result<PullResult> {
        let result = Command::new("git")
            .args(["pull", "--rebase"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|_| DotlingError::Git("git is not installed or not in PATH".to_string()))?;

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);

        if !result.status.success() {
            let combined = format!("{stdout}{stderr}");
            if combined.contains("CONFLICT") || combined.contains("conflict") {
                return Ok(PullResult::Conflict);
            }
            return Err(DotlingError::Git(stderr.trim().to_string()));
        }

        if stdout.contains("Already up to date") || stdout.contains("Already up-to-date") {
            return Ok(PullResult::UpToDate);
        }

        // Count changed files from output
        let file_count = stdout
            .lines()
            .filter(|l| l.contains('|') || l.starts_with(' '))
            .count();
        Ok(PullResult::Updated(file_count.max(1)))
    }

    /// Pushes to the remote.
    pub fn push(&self) -> Result<()> {
        let branch = self.current_branch()?;
        self.run(&["push", "-u", "origin", &branch])?;
        Ok(())
    }

    /// Returns the current branch name.
    pub fn current_branch(&self) -> Result<String> {
        let output = self.run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        Ok(output.trim().to_string())
    }

    /// Checks whether any remote is configured.
    pub fn has_remote(&self) -> Result<bool> {
        let output = self.run(&["remote"])?;
        Ok(!output.trim().is_empty())
    }

    /// Returns a list of changed (uncommitted) files.
    #[allow(dead_code)]
    pub fn changed_files(&self) -> Result<Vec<String>> {
        let output = self.run(&["status", "--porcelain"])?;
        let files: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l[3..].to_string())
            .collect();
        Ok(files)
    }

    /// Clones a remote repository to the given destination.
    pub fn clone(url: &str, dest: &Path) -> Result<()> {
        let dest_str = dest.to_string_lossy();
        let output = Command::new("git")
            .args(["clone", url, &dest_str])
            .output()
            .map_err(|_| DotlingError::Git("git is not installed or not in PATH".to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DotlingError::Git(stderr.trim().to_string()));
        }
        Ok(())
    }
}

/// Runs a git command at the given working directory.
fn run_git_at(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|_| DotlingError::Git("git is not installed or not in PATH".to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DotlingError::Git(stderr.trim().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}
