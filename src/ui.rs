use std::io::{self, BufRead, Write};

// ── ANSI color codes ──────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";

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
fn paint(color: &str, text: &str) -> String {
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
            Self::Conflict => "⚡",
        }
    }

    fn color(&self) -> &str {
        match self {
            Self::Ok | Self::Encrypted => GREEN,
            Self::Modified => YELLOW,
            Self::Missing | Self::Broken => RED,
            Self::Conflict => MAGENTA,
        }
    }
}

/// Print a status line for an entry.
pub fn status_line(status: &Status, source: &str, target: &str) {
    let sym = paint(status.color(), status.symbol());
    let src = paint(CYAN, source);
    let arrow = paint(DIM, "→");
    println!("  {sym} {src} {arrow} {target}");
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
