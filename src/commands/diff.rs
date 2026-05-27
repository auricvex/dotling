/// Show diff between repo source and deployed file.
///
/// Implements a minimal line-by-line unified diff inline (no diff crate).
/// Shows 2 lines of context. Colours added lines green, removed red,
/// context dim.
use std::{fs, path::Path};

use owo_colors::OwoColorize;

use crate::{
    config::Config,
    error::{Result, io_err},
    linker::{EntryStatus, Linker},
    printer::Printer,
    repo,
};

/// Runs the `diff` command.
pub fn run(printer: &Printer, file: Option<&Path>) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let config = Config::load(&repo_root)?;
    let linker = Linker::new(repo_root.clone());

    if config.entries.is_empty() {
        printer.annotation("No tracked entries.");
        return Ok(());
    }

    let mut found = false;

    for entry in config.active_entries() {
        let dest_path = repo::src_to_dest_path(&entry.dest)?;
        let src_path = repo_root.join(&entry.src);

        // If a specific file is requested, filter
        if let Some(target) = file {
            let target_abs = repo::resolve_path(target)?;
            if dest_path != target_abs && src_path != target_abs {
                continue;
            }
        }

        let status = linker.check_entry(entry)?;
        if status != EntryStatus::Modified {
            continue;
        }

        found = true;

        let src_content = fs::read_to_string(&src_path).map_err(io_err(&src_path))?;
        let dest_content = fs::read_to_string(&dest_path).map_err(io_err(&dest_path))?;

        printer.group_header(&format!("{} → {}", entry.src, dest_path.display()));

        print_diff(&src_content, &dest_content);
    }

    if !found {
        if file.is_some() {
            printer.annotation("No modifications found for the specified file.");
        } else {
            printer.annotation("No modifications found.");
        }
    }

    Ok(())
}

/// Prints a minimal unified diff between two strings with 2 lines of context.
fn print_diff(old: &str, new: &str) {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let edits = compute_diff(&old_lines, &new_lines);
    let hunks = group_into_hunks(&edits, 2);

    for hunk in &hunks {
        for edit in hunk {
            match edit {
                DiffEdit::Equal(line) => {
                    println!("  {}", format!(" {line}").dimmed());
                }
                DiffEdit::Delete(line) => {
                    println!("  {}", format!("-{line}").red());
                }
                DiffEdit::Insert(line) => {
                    println!("  {}", format!("+{line}").green());
                }
            }
        }
        println!();
    }
}

/// A single diff edit operation.
#[derive(Debug, Clone)]
enum DiffEdit<'a> {
    /// Line is the same in both files.
    Equal(&'a str),
    /// Line was removed from old.
    Delete(&'a str),
    /// Line was added in new.
    Insert(&'a str),
}

/// Computes a simple longest-common-subsequence based diff.
fn compute_diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffEdit<'a>> {
    let n = old.len();
    let m = new.len();

    // Build LCS table
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if old[i - 1] == new[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to produce edits
    let mut edits = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            edits.push(DiffEdit::Equal(old[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            edits.push(DiffEdit::Insert(new[j - 1]));
            j -= 1;
        } else {
            edits.push(DiffEdit::Delete(old[i - 1]));
            i -= 1;
        }
    }

    edits.reverse();
    edits
}

/// Groups diff edits into hunks with the given number of context lines.
fn group_into_hunks<'a>(edits: &[DiffEdit<'a>], context: usize) -> Vec<Vec<DiffEdit<'a>>> {
    if edits.is_empty() {
        return Vec::new();
    }

    // Find indices of changed lines
    let changed: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, e)| !matches!(e, DiffEdit::Equal(_)))
        .map(|(i, _)| i)
        .collect();

    if changed.is_empty() {
        return Vec::new();
    }

    let mut hunks: Vec<Vec<DiffEdit<'a>>> = Vec::new();
    let mut current_hunk: Vec<DiffEdit<'a>> = Vec::new();
    let mut hunk_end: usize = 0;

    for (idx, &change_idx) in changed.iter().enumerate() {
        let start = change_idx.saturating_sub(context);
        let end = (change_idx + context + 1).min(edits.len());

        if idx == 0 || start > hunk_end {
            // Start a new hunk
            if !current_hunk.is_empty() {
                hunks.push(current_hunk);
            }
            current_hunk = Vec::new();
            for edit in &edits[start..end] {
                current_hunk.push(edit.clone());
            }
        } else {
            // Extend current hunk
            let extend_start = hunk_end;
            for edit in &edits[extend_start..end] {
                current_hunk.push(edit.clone());
            }
        }
        hunk_end = end;
    }

    if !current_hunk.is_empty() {
        hunks.push(current_hunk);
    }

    hunks
}
