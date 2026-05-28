//! Backup subsystem.
//!
//! Before dotling overwrites any existing local file it calls [`backup`], which
//! atomically copies the file to:
//!
//! ```text
//! ~/.dotling/backups/<ISO-8601-timestamp>/<repo-relative-source-path>
//! ```
//!
//! The `dotling backup clean` command calls [`clean`] to prune old sessions.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    error::{Error, Result},
    store,
};

// ── Write ─────────────────────────────────────────────────────────

/// Backup `target` (the live file on disk) before it is overwritten.
///
/// `source_key` is the repo-relative source path (e.g. `shell/fish/config.fish`)
/// used to construct the backup file's path inside the backup session directory.
///
/// Returns the full path of the written backup file so the caller can display it.
pub fn backup(target: &Path, source_key: &str) -> Result<PathBuf> {
    let session_dir = current_session_dir()?;
    // Mirror the source key as a sub-path inside the session directory.
    let dest = session_dir.join(source_key);

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, "create backup directory", e))?;
    }

    crate::fs::copy_file(target, &dest)?;
    Ok(dest)
}

/// Backup a directory recursively.
pub fn backup_dir(target: &Path, source_key: &str) -> Result<PathBuf> {
    let session_dir = current_session_dir()?;
    let dest = session_dir.join(source_key);
    copy_dir_recursive(target, &dest)?;
    Ok(dest)
}

// ── Clean ─────────────────────────────────────────────────────────

/// List all backup sessions (sorted oldest → newest).
pub fn list_sessions() -> Result<Vec<PathBuf>> {
    let dir = store::backup_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| Error::io(&dir, "read backup directory", e))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();

    // Sort lexicographically — ISO timestamps sort chronologically.
    sessions.sort();
    Ok(sessions)
}

/// Remove old backup sessions according to the given policy.
///
/// - `keep_last`: if `Some(n)`, keep the `n` most recent sessions.
/// - `older_than_days`: if `Some(d)`, delete sessions older than `d` days.
///
/// Both filters are applied independently; a session is removed if **either**
/// condition marks it for deletion.
pub fn clean(keep_last: Option<usize>, older_than_days: Option<u64>) -> Result<CleanSummary> {
    let sessions = list_sessions()?;
    let total = sessions.len();

    // Determine the set to remove.
    let mut to_remove: Vec<&PathBuf> = Vec::new();

    if let Some(keep) = keep_last {
        let cutoff = total.saturating_sub(keep);
        for s in sessions.iter().take(cutoff) {
            if !to_remove.contains(&s) {
                to_remove.push(s);
            }
        }
    }

    if let Some(days) = older_than_days {
        let threshold_secs = days * 86_400;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());

        for s in &sessions {
            let ts = session_timestamp_secs(s);
            if ts > 0 && now.saturating_sub(ts) > threshold_secs && !to_remove.contains(&s) {
                to_remove.push(s);
            }
        }
    }

    let removed_count = to_remove.len();
    for path in to_remove {
        fs::remove_dir_all(path).map_err(|e| Error::io(path, "remove backup session", e))?;
    }

    Ok(CleanSummary {
        total,
        removed: removed_count,
    })
}

/// Summary returned by [`clean`].
#[derive(Clone, Copy)]
pub struct CleanSummary {
    pub total: usize,
    pub removed: usize,
}

// ── Internal helpers ──────────────────────────────────────────────

/// Return (and lazily create) the directory for the *current* backup session.
///
/// The session name is `<unix-seconds>` which sorts well and is unique enough
/// for interactive use.  We do not use the full ISO timestamp to avoid colon
/// characters in path names on Windows.
fn current_session_dir() -> Result<PathBuf> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let dir = store::backup_dir()?.join(format!("{ts}"));
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, "create backup session directory", e))?;
    Ok(dir)
}

/// Parse a session directory name back to Unix seconds (best-effort).
fn session_timestamp_secs(path: &Path) -> u64 {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Recursively copy `src` → `dst`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, "create directory", e))?;
    for entry in fs::read_dir(src).map_err(|e| Error::io(src, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(src, "read directory entry", e))?;
        let sp = entry.path();
        let dp = dst.join(entry.file_name());
        if sp.is_dir() {
            copy_dir_recursive(&sp, &dp)?;
        } else {
            crate::fs::copy_file(&sp, &dp)?;
        }
    }
    Ok(())
}
