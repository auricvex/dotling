/// Crate-wide error types for dotling.
///
/// All errors propagate through the [`DotlingError`] enum and are surfaced
/// to the user via the [`crate::printer::Printer`] module. No panics in
/// user-facing code.
use std::path::PathBuf;

/// Central error type for all dotling operations.
#[derive(Debug, thiserror::Error)]
pub enum DotlingError {
    /// The dotling repository has not been initialized yet.
    #[error("dotling repo not found — run `dotling init <path>` first")]
    RepoNotFound,

    /// A repository already exists at the given path.
    #[error("already initialized at {0}")]
    AlreadyInitialized(PathBuf),

    /// The specified path does not exist on disk.
    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    /// The path is outside the user's home directory.
    #[error("path is outside home directory: {0}")]
    PathOutsideHome(PathBuf),

    /// The file is already tracked by dotling.
    #[error("already tracked: {0}")]
    AlreadyTracked(PathBuf),

    /// The file is not currently tracked by dotling.
    #[error("not tracked: {0}")]
    NotTracked(PathBuf),

    /// An unmanaged file exists at the link target destination.
    #[error("destination conflict — unmanaged file exists at {0}")]
    DestinationConflict(PathBuf),

    /// A git command exited with a non-zero status.
    #[error("git: {0}")]
    Git(String),

    /// An I/O error occurred, with the associated path for context.
    #[error("{path}: {source}")]
    Io {
        /// The path involved in the I/O operation.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// The config file could not be parsed.
    #[error("failed to parse config: {0}")]
    ConfigParse(String),

    /// The config file could not be written.
    #[error("failed to write config: {0}")]
    #[allow(dead_code)]
    ConfigWrite(String),

    /// No git remote is configured on the repository.
    #[error("no git remote configured — add one with `git remote add origin <url>`")]
    NoRemote,

    /// The path is already a symlink (cannot link a symlink).
    #[error("already a symlink: {0}")]
    AlreadySymlink(PathBuf),
}

/// Returns a closure that converts [`std::io::Error`] into
/// [`DotlingError::Io`] with the given path attached.
///
/// # Usage
///
/// ```ignore
/// std::fs::read(&path).map_err(io_err(&path))?;
/// ```
pub fn io_err(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> DotlingError + '_ {
    move |source| DotlingError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Crate-wide result alias using [`DotlingError`].
pub type Result<T> = std::result::Result<T, DotlingError>;
