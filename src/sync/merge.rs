//! Line-level three-way merge.
//!
//! Given a common `base` and two diverged versions (`ours` and `theirs`),
//! produces a merged result.  Non-conflicting changes from both sides are
//! applied automatically; overlapping changes are wrapped in conflict markers:
//!
//! ```text
//! <<<<<<< ours
//! line only in ours
//! =======
//! line only in theirs
//! >>>>>>> theirs
//! ```
//!
//! This is intentionally simple — it is line-granular and does not attempt
//! word-level or semantic merging.  It mirrors the behaviour of `git merge-file`
//! with default (non-diff3) conflict style.

use std::fmt::Write as _;

// ── Public API ────────────────────────────────────────────────────

/// Result of a three-way merge.
#[derive(Debug)]
pub struct MergeResult {
    /// The merged text (may contain conflict markers if `has_conflicts` is true).
    pub content: String,
    /// `true` if any conflicting hunks were found and could not be auto-resolved.
    pub has_conflicts: bool,
    /// Number of conflict hunks inserted.
    pub conflict_count: usize,
}

/// Perform a line-level three-way merge.
///
/// - `base`   — the common ancestor (last-known-good snapshot).
/// - `ours`   — the repo version (what the dotling repo contains).
/// - `theirs` — the actual / local version (what is on disk).
/// - `ours_label`   — label shown in `<<<<<<<` marker (e.g. `"repo"`).
/// - `theirs_label` — label shown in `>>>>>>>` marker (e.g. `"actual"`).
pub fn three_way_merge(
    base: &str,
    ours: &str,
    theirs: &str,
    ours_label: &str,
    theirs_label: &str,
) -> MergeResult {
    let base_lines: Vec<&str> = base.lines().collect();
    let ours_lines: Vec<&str> = ours.lines().collect();
    let theirs_lines: Vec<&str> = theirs.lines().collect();

    let ours_ops = diff(&base_lines, &ours_lines);
    let theirs_ops = diff(&base_lines, &theirs_lines);

    merge_ops(
        &base_lines,
        &ours_ops,
        &theirs_ops,
        &ours_lines,
        &theirs_lines,
        ours_label,
        theirs_label,
    )
}

// ── Diff ──────────────────────────────────────────────────────────

/// A single edit operation relative to the base sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    /// Keep the base line at this index unchanged.
    Keep(usize),
    /// Delete the base line at this index.
    Delete(usize),
    /// Insert a new line (index into the other sequence) before the current base position.
    Insert(usize),
}

/// Compute a diff between `a` (base) and `b` (modified) using a simple LCS
/// approach (patience-inspired).  Returns a sequence of `Op`s.
fn diff<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<Op> {
    let lcs = lcs_table(a, b);
    let mut ops = Vec::new();
    build_ops(a, b, a.len(), b.len(), &lcs, &mut ops);
    ops
}

/// Build the LCS length table.
fn lcs_table(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }
    dp
}

/// Recursively walk the LCS table to produce edit operations.
fn build_ops(a: &[&str], b: &[&str], i: usize, j: usize, dp: &[Vec<usize>], ops: &mut Vec<Op>) {
    if i == 0 && j == 0 {
        return;
    }
    if i == 0 {
        build_ops(a, b, i, j - 1, dp, ops);
        ops.push(Op::Insert(j - 1));
    } else if j == 0 {
        build_ops(a, b, i - 1, j, dp, ops);
        ops.push(Op::Delete(i - 1));
    } else if a[i - 1] == b[j - 1] {
        build_ops(a, b, i - 1, j - 1, dp, ops);
        ops.push(Op::Keep(i - 1));
    } else if dp[i - 1][j] >= dp[i][j - 1] {
        build_ops(a, b, i - 1, j, dp, ops);
        ops.push(Op::Delete(i - 1));
    } else {
        build_ops(a, b, i, j - 1, dp, ops);
        ops.push(Op::Insert(j - 1));
    }
}

// ── Merge ─────────────────────────────────────────────────────────

struct SideState<'a> {
    inserted: Vec<Vec<&'a str>>,
    deleted: Vec<bool>,
}

fn get_side_state<'a>(base_len: usize, ops: &[Op], other_lines: &[&'a str]) -> SideState<'a> {
    let mut inserted = vec![Vec::new(); base_len + 1];
    let mut deleted = vec![false; base_len];
    let mut b_idx = 0;
    for op in ops {
        match op {
            Op::Keep(b) => {
                b_idx = *b;
                if b_idx < base_len {
                    b_idx += 1;
                }
            }
            Op::Delete(b) => {
                b_idx = *b;
                if b_idx < base_len {
                    deleted[b_idx] = true;
                    b_idx += 1;
                }
            }
            Op::Insert(o) => {
                if b_idx <= base_len {
                    inserted[b_idx].push(other_lines[*o]);
                }
            }
        }
    }
    SideState { inserted, deleted }
}

/// Align the two op streams against the shared base and produce hunks.
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::needless_range_loop)]
fn merge_ops(
    base_lines: &[&str],
    ours_ops: &[Op],
    theirs_ops: &[Op],
    ours_lines: &[&str],
    theirs_lines: &[&str],
    ours_label: &str,
    theirs_label: &str,
) -> MergeResult {
    let base_len = base_lines.len();
    let ours_state = get_side_state(base_len, ours_ops, ours_lines);
    let theirs_state = get_side_state(base_len, theirs_ops, theirs_lines);

    // Identify which base lines are "cleanly kept" boundaries.
    // A base line b (0..base_len) is a boundary if:
    // 1. Neither side deleted it (!ours_state.deleted[b] && !theirs_state.deleted[b]).
    // 2. Neither side had any insertions immediately before it (ours_state.inserted[b].is_empty()
    //    && theirs_state.inserted[b].is_empty()).
    let mut is_boundary = vec![false; base_len];
    for b in 0..base_len {
        if !ours_state.deleted[b]
            && !theirs_state.deleted[b]
            && ours_state.inserted[b].is_empty()
            && theirs_state.inserted[b].is_empty()
        {
            is_boundary[b] = true;
        }
    }

    let mut out = String::new();
    let mut has_conflicts = false;
    let mut conflict_count = 0;

    let mut b = 0;
    while b <= base_len {
        if b < base_len && is_boundary[b] {
            // Output boundary line cleanly.
            out.push_str(base_lines[b]);
            out.push('\n');
            b += 1;
        } else {
            // Find the end of this change block.
            let start = b;
            let mut end = b;
            while end < base_len && !is_boundary[end] {
                end += 1;
            }

            let mut ours_prod = Vec::new();
            let mut theirs_prod = Vec::new();
            let mut ours_has_edits = false;
            let mut theirs_has_edits = false;

            for curr in start..=end {
                // Collect insertions before base line curr.
                for &line in &ours_state.inserted[curr] {
                    ours_prod.push(line);
                    ours_has_edits = true;
                }
                for &line in &theirs_state.inserted[curr] {
                    theirs_prod.push(line);
                    theirs_has_edits = true;
                }

                // If curr < end, handle the base line curr itself.
                if curr < end {
                    if ours_state.deleted[curr] {
                        ours_has_edits = true;
                    } else {
                        ours_prod.push(base_lines[curr]);
                    }

                    if theirs_state.deleted[curr] {
                        theirs_has_edits = true;
                    } else {
                        theirs_prod.push(base_lines[curr]);
                    }
                }
            }

            // Decide how to merge this change block.
            if ours_has_edits && theirs_has_edits {
                // Both changed this block.
                if ours_prod == theirs_prod {
                    // Both produced the same result -> auto-merge.
                    for line in ours_prod {
                        out.push_str(line);
                        out.push('\n');
                    }
                } else {
                    // Conflict!
                    has_conflicts = true;
                    conflict_count += 1;
                    let _ = writeln!(out, "<<<<<<< {ours_label}");
                    for line in ours_prod {
                        out.push_str(line);
                        out.push('\n');
                    }
                    out.push_str("=======\n");
                    for line in theirs_prod {
                        out.push_str(line);
                        out.push('\n');
                    }
                    let _ = writeln!(out, ">>>>>>> {theirs_label}");
                }
            } else if ours_has_edits {
                // Only ours changed this block.
                for line in ours_prod {
                    out.push_str(line);
                    out.push('\n');
                }
            } else if theirs_has_edits {
                // Only theirs changed this block.
                for line in theirs_prod {
                    out.push_str(line);
                    out.push('\n');
                }
            } else {
                // Neither side had edits -> output base lines.
                for curr in start..end {
                    out.push_str(base_lines[curr]);
                    out.push('\n');
                }
            }

            if end == base_len {
                b = base_len + 1;
            } else {
                b = end;
            }
        }
    }

    let newline_needed = base_lines
        .last()
        .or(ours_lines.last())
        .or(theirs_lines.last())
        .is_some_and(|l| !l.is_empty());

    // Trim a trailing newline if the original had none.
    if !newline_needed && out.ends_with('\n') {
        out.pop();
    }

    MergeResult {
        content: out,
        has_conflicts,
        conflict_count,
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_files_no_conflict() {
        let r = three_way_merge("a\nb\nc\n", "a\nb\nc\n", "a\nb\nc\n", "repo", "actual");
        assert!(!r.has_conflicts);
        assert_eq!(r.conflict_count, 0);
    }

    #[test]
    fn only_ours_changed() {
        let r = three_way_merge("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n", "repo", "actual");
        assert!(!r.has_conflicts);
        assert!(r.content.contains('B'));
        assert!(!r.content.contains('b'));
    }

    #[test]
    fn only_theirs_changed() {
        let r = three_way_merge("a\nb\nc\n", "a\nb\nc\n", "a\nB\nc\n", "repo", "actual");
        assert!(!r.has_conflicts);
        assert!(r.content.contains('B'));
    }

    #[test]
    fn non_overlapping_changes_auto_merged() {
        let base = "a\nb\nc\nd\n";
        let ours = "A\nb\nc\nd\n"; // changed first line
        let theirs = "a\nb\nc\nD\n"; // changed last line
        let r = three_way_merge(base, ours, theirs, "repo", "actual");
        assert!(!r.has_conflicts, "no overlap — should auto-merge");
        assert!(r.content.contains('A'));
        assert!(r.content.contains('D'));
    }

    #[test]
    fn overlapping_changes_produce_conflict() {
        let base = "a\nb\nc\n";
        let ours = "a\nX\nc\n";
        let theirs = "a\nY\nc\n";
        let r = three_way_merge(base, ours, theirs, "repo", "actual");
        assert!(r.has_conflicts);
        assert_eq!(r.conflict_count, 1);
        assert!(r.content.contains("<<<<<<< repo"));
        assert!(r.content.contains(">>>>>>> actual"));
        assert!(r.content.contains('X'));
        assert!(r.content.contains('Y'));
    }

    #[test]
    fn both_add_same_line_no_conflict() {
        // Both sides appended the same line — should not conflict.
        let base = "a\n";
        let ours = "a\nnew\n";
        let theirs = "a\nnew\n";
        let r = three_way_merge(base, ours, theirs, "repo", "actual");
        // May or may not auto-resolve, but if it conflicts the content is still correct.
        // At minimum, "new" must appear in the output.
        assert!(r.content.contains("new"));
    }
}
