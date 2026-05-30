use std::io::{self, BufRead, Write};

// ── ANSI color codes ──────────────────────────────────────────────

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";

/// Returns `true` if the terminal supports color output.
///
/// Respects the `NO_COLOR` environment variable (see <https://no-color.org>).
fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    // Check if stdout is a TTY (on Unix)
    #[cfg(unix)]
    {
        unsafe { libc_isatty(1) }
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> bool {
    unsafe { libc_isatty_inner(fd) != 0 }
}

#[cfg(unix)]
unsafe fn libc_isatty_inner(fd: i32) -> i32 {
    unsafe extern "C" {
        fn isatty(fd: i32) -> i32;
    }
    unsafe { isatty(fd) }
}

/// Apply a color if colors are enabled.
pub fn paint(color: &str, text: &str) -> String {
    if color_enabled() {
        format!("{color}{text}{RESET}")
    } else {
        text.to_string()
    }
}

// ── Public output helpers ─────────────────────────────────────────

/// Print a success message: `✓ <message>`
pub fn success(msg: &str) {
    println!("  {} {msg}", paint(GREEN, "✓"));
}

/// Print an error message: `✗ <message>`
pub fn error(msg: &str) {
    eprintln!("  {} {msg}", paint(RED, "✗"));
}

/// Print a warning message: `⚠ <message>`
pub fn warning(msg: &str) {
    eprintln!("  {} {msg}", paint(YELLOW, "⚠"));
}

/// Print an info message: `· <message>`
pub fn info(msg: &str) {
    println!("  {} {msg}", paint(BLUE, "·"));
}

/// Print a hint (verbose-only) message: `  <message>`
pub fn hint(msg: &str) {
    println!("    {}", paint(DIM, msg));
}

/// Print a section header.
pub fn header(title: &str) {
    println!("\n{}", paint(BOLD, title));
}

/// Print a dimmed line.
pub fn dim(msg: &str) {
    println!("  {}", paint(DIM, msg));
}

// ── Status display ────────────────────────────────────────────────

/// Status indicator for an entry.
pub enum Status {
    Ok,
    Modified,
    Missing,
    Broken,
    Encrypted,
    Template,
    Conflict,
}

impl Status {
    fn symbol(&self) -> &str {
        match self {
            Self::Ok => "✓",
            Self::Modified => "~",
            Self::Missing => "✗",
            Self::Broken => "!",
            Self::Encrypted => "🔒",
            Self::Template => "📄",
            Self::Conflict => "⚡",
        }
    }

    fn color(&self) -> &str {
        match self {
            Self::Ok | Self::Encrypted | Self::Template => GREEN,
            Self::Modified => YELLOW,
            Self::Missing | Self::Broken => RED,
            Self::Conflict => MAGENTA,
        }
    }
}

/// Sync and diff state badges shown alongside each status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncBadge {
    /// Entry is fully in sync — no action needed.
    InSync,
    /// Entry needs to be synced (missing, broken, conflict, or symlink wrong).
    NeedsSync,
    /// Entry content differs between repo and actual (copy-mode modified).
    HasDiff,
}

impl SyncBadge {
    fn render(self) -> String {
        match self {
            Self::InSync => paint(GREEN, "[in sync]"),
            Self::NeedsSync => paint(YELLOW, "[needs sync]"),
            Self::HasDiff => format!(
                "{} {}",
                paint(YELLOW, "[needs sync]"),
                paint(MAGENTA, "[diff]")
            ),
        }
    }
}

/// Print a status line for an entry, with an optional sync badge.
pub fn status_line(status: &Status, source: &str, target: &str, badge: SyncBadge) {
    let sym = paint(status.color(), status.symbol());
    let src = paint(CYAN, source);
    let arrow = paint(DIM, "→");
    let badge_str = badge.render();
    println!("  {sym} {src} {arrow} {target}  {badge_str}");
}

// ── Summaries ─────────────────────────────────────────────────────

/// Print a summary line after a batch operation.
pub fn summary(ok: usize, warnings: usize, errors: usize) {
    let parts: Vec<String> = [
        (ok, "ok", GREEN),
        (warnings, "warning", YELLOW),
        (errors, "error", RED),
    ]
    .iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, label, color)| {
        let plural = if *n == 1 { "" } else { "s" };
        paint(color, &format!("{n} {label}{plural}"))
    })
    .collect();

    if parts.is_empty() {
        println!("\n  {}", paint(DIM, "nothing to do"));
    } else {
        println!("\n  {}", parts.join(", "));
    }
}

// ── Interactive prompts ───────────────────────────────────────────

/// Ask a yes/no question. Returns `true` for yes.
pub fn confirm(question: &str) -> bool {
    print!("  {} {question} [y/N] ", paint(YELLOW, "?"));
    io::stdout().flush().ok();

    let mut input = String::new();
    if io::stdin().lock().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Prompt for a password (input hidden on Unix).
pub fn password(question: &str) -> String {
    print!("  {} {question}: ", paint(YELLOW, "🔑"));
    io::stdout().flush().ok();

    #[cfg(unix)]
    {
        // Disable echo
        if let Some(pwd) = read_password_unix() {
            println!(); // newline after hidden input
            return pwd;
        }
    }

    // Fallback: visible input
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input).ok();
    input.trim().to_string()
}

#[cfg(unix)]
fn read_password_unix() -> Option<String> {
    use std::os::unix::io::AsRawFd;

    let stdin = io::stdin();
    let fd = stdin.as_raw_fd();

    unsafe {
        let mut termios = std::mem::zeroed::<Termios>();
        if tcgetattr(fd, &raw mut termios) != 0 {
            return None;
        }

        let mut noecho = termios;
        noecho.c_lflag &= !ECHO;

        if tcsetattr(fd, 0, &raw const noecho) != 0 {
            return None;
        }

        let mut input = String::new();
        let result = stdin.lock().read_line(&mut input);

        // Restore terminal
        tcsetattr(fd, 0, &raw const termios);

        result.ok()?;
        Some(input.trim().to_string())
    }
}

#[cfg(unix)]
const ECHO: u64 = 0x0000_0008;

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::struct_field_names)]
struct Termios {
    c_iflag: u64,
    c_oflag: u64,
    c_cflag: u64,
    c_lflag: u64,
    c_cc: [u8; 20],
    c_ispeed: u64,
    c_ospeed: u64,
}

#[cfg(unix)]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios: *const Termios) -> i32;
}

// ── Diff display ──────────────────────────────────────────────────

/// Print a unified-diff-style comparison between two texts.
pub fn print_diff(source_label: &str, target_label: &str, source: &str, target: &str) {
    let source_lines: Vec<&str> = source.lines().collect();
    let target_lines: Vec<&str> = target.lines().collect();

    println!("{} {source_label}", paint(RED, "---"));
    println!("{} {target_label}", paint(GREEN, "+++"));

    // Simple line-by-line diff (no LCS — fast and good enough for config files)
    let max = source_lines.len().max(target_lines.len());
    for i in 0..max {
        match (source_lines.get(i), target_lines.get(i)) {
            (Some(a), Some(b)) if a == b => {
                println!(" {a}");
            }
            (Some(a), Some(b)) => {
                println!("{}", paint(RED, &format!("-{a}")));
                println!("{}", paint(GREEN, &format!("+{b}")));
            }
            (Some(a), None) => {
                println!("{}", paint(RED, &format!("-{a}")));
            }
            (None, Some(b)) => {
                println!("{}", paint(GREEN, &format!("+{b}")));
            }
            (None, None) => {}
        }
    }
}

// ── Conflict resolution UI ────────────────────────────────────────

/// Print a conflict header for a single entry.
///
/// `origin_label` should be a short human-readable string like
/// `"first-seen"`, `"both-modified"`, or `"ambiguous timestamp"`.
pub fn conflict_header(origin_label: &str, source: &str, target: &str) {
    println!("\n  {} conflict ({origin_label}):", paint(MAGENTA, "⚡"));
    println!("    {} {} {}", paint(CYAN, source), paint(DIM, "↔"), target);
}

/// The user's response to a conflict resolution prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    /// Keep the local (actual) file — pull it into the repo.
    KeepLocal,
    /// Use the repo version — push to actual (local file will be backed up).
    UseRepo,
    /// Attempt an automatic 3-way merge (only for plain-text files).
    Merge,
    /// Skip this entry for now; leave both sides unchanged.
    Skip,
    /// Show a diff between repo and actual before deciding.
    ShowDiff,
}

/// Interactive conflict resolution prompt.
///
/// Returns the user's choice.  If stdin is not a TTY (e.g. piped), defaults
/// to `Skip`.
pub fn conflict_prompt(supports_merge: bool) -> ConflictChoice {
    let merge_hint = if supports_merge { " [m]erge" } else { "" };
    loop {
        print!(
            "    {} [k]eep local  [r]epo{merge_hint}  [d]iff  [s]kip > ",
            paint(YELLOW, "?"),
        );
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().lock().read_line(&mut input).is_err() || input.is_empty() {
            return ConflictChoice::Skip;
        }

        match input.trim().to_ascii_lowercase().as_str() {
            "k" | "keep" | "keep-local" | "keeplocal" => return ConflictChoice::KeepLocal,
            "r" | "repo" => return ConflictChoice::UseRepo,
            "m" | "merge" if supports_merge => return ConflictChoice::Merge,
            "d" | "diff" => return ConflictChoice::ShowDiff,
            "s" | "skip" | "" => return ConflictChoice::Skip,
            _ => {
                println!("    {}", paint(DIM, "unrecognised — type k, r, m, d, or s"));
            }
        }
    }
}

/// Print a notice that a 3-way merge succeeded with conflict markers.
pub fn merge_conflict_notice(conflict_count: usize, path: &std::path::Path) {
    println!(
        "    {} {conflict_count} conflict hunk(s) need manual resolution in {}",
        paint(YELLOW, "⚠"),
        paint(CYAN, &path.display().to_string()),
    );
}

/// Print a notice that a 3-way merge was clean (no conflict markers).
pub fn merge_clean_notice(path: &std::path::Path) {
    println!(
        "    {} merged cleanly → {}",
        paint(GREEN, "✓"),
        paint(CYAN, &path.display().to_string()),
    );
}
