/// Audit repository health and report issues.
///
/// Checks: repo discovery, config parsing, git remote configuration,
/// each entry's status (source exists + deployment status), and orphan
/// files in the repo not referenced by config.
use std::collections::HashSet;

use walkdir::WalkDir;

use crate::{
    config::{CONFIG_FILE, Config},
    git::Git,
    linker::{EntryStatus, Linker},
    platform::Platform,
    printer::Printer,
    repo,
};

/// Mutable counters for doctor diagnostics.
struct Counters {
    /// Number of errors found.
    errors: usize,
    /// Number of warnings found.
    warnings: usize,
}

/// Runs the `doctor` command.
pub fn run(printer: &Printer) {
    let mut c = Counters {
        errors: 0,
        warnings: 0,
    };

    // 1. Check repo discovery
    printer.group_header("Repository");
    let repo_root = match repo::get_repo_root() {
        Ok(root) => {
            printer.ok("found", &root);
            root
        }
        Err(e) => {
            printer.error_msg(&format!("repo: {e}"));
            c.errors += 1;
            print_doctor_summary(printer, &c);
            return;
        }
    };

    // 2. Check config
    printer.group_header("Configuration");
    let config = match Config::load(&repo_root) {
        Ok(config) => {
            let config_path = repo_root.join(CONFIG_FILE);
            printer.ok("parsed", &config_path);
            config
        }
        Err(e) => {
            printer.error_msg(&format!("config: {e}"));
            c.errors += 1;
            print_doctor_summary(printer, &c);
            return;
        }
    };

    check_git(printer, &repo_root, &mut c);
    check_entries(printer, &repo_root, &config, &mut c);
    check_orphans(printer, &repo_root, &config, &mut c);
    print_doctor_summary(printer, &c);
}

/// Checks git remote configuration.
fn check_git(printer: &Printer, repo_root: &std::path::Path, c: &mut Counters) {
    printer.group_header("Git");
    let git = Git::new(repo_root.to_path_buf());
    match git.has_remote() {
        Ok(true) => {
            printer.ok("remote", repo_root);
        }
        Ok(false) => {
            printer.warn("no remote", "no git remote configured");
            printer.hint("Add one with: git remote add origin <url>");
            c.warnings += 1;
        }
        Err(e) => {
            printer.error_msg(&format!("git: {e}"));
            c.errors += 1;
        }
    }
}

/// Checks each tracked entry's status.
fn check_entries(
    printer: &Printer,
    repo_root: &std::path::Path,
    config: &Config,
    c: &mut Counters,
) {
    printer.group_header("Entries");
    let linker = Linker::new(repo_root.to_path_buf());

    for entry in &config.entries {
        // Skip entries for other OSes with an informational note
        if !entry.os.is_active() {
            let label = format!("skip [{}]", entry.os);
            printer.skipped(&label, std::path::Path::new(&entry.dest));
            continue;
        }

        let os_suffix = if entry.os == Platform::All {
            String::new()
        } else {
            format!(" [{}]", entry.os)
        };

        let dest_path = match repo::src_to_dest_path(&entry.dest) {
            Ok(p) => p,
            Err(e) => {
                printer.error_msg(&format!("{}: {e}", entry.dest));
                c.errors += 1;
                continue;
            }
        };

        let src_path = repo_root.join(&entry.src);
        if !src_path.exists() {
            printer.error_line("no src", &src_path);
            c.errors += 1;
            continue;
        }

        match linker.check_entry(entry) {
            Ok(EntryStatus::Ok) => {
                let label = format!("ok{os_suffix}");
                printer.ok(&label, &dest_path);
            }
            Ok(EntryStatus::Modified) => {
                printer.skipped("modified", &dest_path);
                c.warnings += 1;
            }
            Ok(EntryStatus::BrokenSymlink) => {
                printer.error_line("broken", &dest_path);
                c.errors += 1;
            }
            Ok(EntryStatus::Missing) => {
                printer.missing(&dest_path);
                c.errors += 1;
            }
            Ok(EntryStatus::Conflict) => {
                printer.error_line("conflict", &dest_path);
                c.warnings += 1;
            }
            Err(e) => {
                printer.error_msg(&format!("{}: {e}", dest_path.display()));
                c.errors += 1;
            }
        }
    }
}

/// Checks for orphan files not referenced by config.
fn check_orphans(
    printer: &Printer,
    repo_root: &std::path::Path,
    config: &Config,
    c: &mut Counters,
) {
    printer.group_header("Orphans");
    let tracked_srcs: HashSet<String> = config.entries.iter().map(|e| e.src.clone()).collect();

    let mut orphan_count = 0usize;
    for entry in WalkDir::new(repo_root)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Ok(rel) = path.strip_prefix(repo_root) {
            let rel_str = rel.to_string_lossy().to_string();
            if rel_str.starts_with(".git") || rel_str == CONFIG_FILE || rel_str.starts_with('.') {
                continue;
            }
            if !tracked_srcs.contains(&rel_str) {
                printer.skipped("orphan", path);
                orphan_count += 1;
                c.warnings += 1;
            }
        }
    }

    if orphan_count == 0 {
        printer.ok("none", std::path::Path::new("no orphan files"));
    }
}

/// Prints the doctor summary.
fn print_doctor_summary(printer: &Printer, c: &Counters) {
    printer.annotation(&format!(
        "\n{} error(s), {} warning(s)",
        c.errors, c.warnings
    ));
    if c.errors == 0 && c.warnings == 0 {
        printer.success("Everything looks good!");
    } else if c.errors == 0 {
        printer.warn_msg("Some warnings found — review above.");
    } else {
        printer.error_msg("Issues found — review above.");
    }
}
