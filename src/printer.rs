/// Terminal output formatting for dotling.
///
/// All user-facing output goes through [`Printer`] to ensure consistent,
/// column-aligned, coloured formatting. No raw `println!` in business logic.
use std::path::Path;

use owo_colors::OwoColorize;

/// Label padding width for column-aligned output.
const LABEL_WIDTH: usize = 8;

/// Prefix for all action lines.
const PREFIX: &str = "  ·  ";

/// Handles all terminal output with consistent formatting.
///
/// The `verbose` field controls whether hint messages are shown.
/// All methods take `&self` to allow future stateful extensions
/// (e.g. output buffering, quiet mode).
pub struct Printer {
    /// Whether to show verbose/hint output.
    pub verbose: bool,
}

#[allow(clippy::unused_self)]
impl Printer {
    /// Creates a new printer with the given verbosity setting.
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Prints a generic action line: `  ·  <label>  <path>`.
    pub fn action(&self, label: &str, path: &Path) {
        println!(
            "{PREFIX}{:>width$}  {}",
            label.blue(),
            path.display(),
            width = LABEL_WIDTH,
        );
    }

    /// Prints an arrow line: `  ·  <label>  <src>  →  <dest>`.
    pub fn arrow(&self, label: &str, src: &Path, dest: &Path) {
        println!(
            "{PREFIX}{:>width$}  {}  →  {}",
            label.blue(),
            src.display(),
            dest.display(),
            width = LABEL_WIDTH,
        );
    }

    /// Prints an ok/success status line (green label).
    pub fn ok(&self, label: &str, path: &Path) {
        println!(
            "{PREFIX}{:>width$}  {}",
            label.green(),
            path.display(),
            width = LABEL_WIDTH,
        );
    }

    /// Prints a skipped/warning status line (yellow label).
    pub fn skipped(&self, label: &str, path: &Path) {
        println!(
            "{PREFIX}{:>width$}  {}",
            label.yellow(),
            path.display(),
            width = LABEL_WIDTH,
        );
    }

    /// Prints a warning status line (yellow label) with a message.
    pub fn warn(&self, label: &str, msg: &str) {
        println!(
            "{PREFIX}{:>width$}  {}",
            label.yellow(),
            msg,
            width = LABEL_WIDTH,
        );
    }

    /// Prints an error status line (red label) with a path.
    pub fn error_line(&self, label: &str, path: &Path) {
        println!(
            "{PREFIX}{:>width$}  {}",
            label.red(),
            path.display(),
            width = LABEL_WIDTH,
        );
    }

    /// Prints a missing status line (red label).
    pub fn missing(&self, path: &Path) {
        println!(
            "{PREFIX}{:>width$}  {}",
            "missing".red(),
            path.display(),
            width = LABEL_WIDTH,
        );
    }

    /// Prints a group header (purple, bold).
    pub fn group_header(&self, name: &str) {
        println!("\n  {}:", name.purple().bold());
    }

    /// Prints a dim annotation line.
    pub fn annotation(&self, msg: &str) {
        println!("  {}", msg.dimmed());
    }

    /// Prints a summary line: `  N ok · N modified · N missing`.
    pub fn summary(&self, ok: usize, modified: usize, missing: usize) {
        println!(
            "\n  {}",
            format!("{ok} ok · {modified} modified · {missing} missing").dimmed(),
        );
    }

    /// Prints a success message (green, bold).
    pub fn success(&self, msg: &str) {
        println!("\n  {}", msg.green().bold());
    }

    /// Prints an error message (red, bold).
    pub fn error_msg(&self, msg: &str) {
        println!("  {}", msg.red().bold());
    }

    /// Prints a warning message (yellow).
    pub fn warn_msg(&self, msg: &str) {
        println!("  {}", msg.yellow());
    }

    /// Prints a hint message (dim), only when verbose mode is enabled.
    pub fn hint(&self, msg: &str) {
        if self.verbose {
            println!("  {}", msg.dimmed());
        }
    }
}
