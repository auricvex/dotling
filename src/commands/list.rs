/// List all tracked entries, grouped by category.
///
/// Groups entries by top-level src directory component. Sorts groups and
/// entries alphabetically. Prints total count.
use std::collections::BTreeMap;

use crate::{config::Config, error::Result, platform::Platform, printer::Printer, repo};

/// Runs the `list` command.
pub fn run(printer: &Printer) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let config = Config::load(&repo_root)?;

    if config.entries.is_empty() {
        printer.annotation("No tracked entries.");
        return Ok(());
    }

    // Group by top-level src directory component
    #[allow(clippy::type_complexity)]
    let mut groups: BTreeMap<String, Vec<(&str, &str, Platform)>> = BTreeMap::new();

    for entry in &config.entries {
        let group = entry.src.split('/').next().unwrap_or("other").to_string();
        groups
            .entry(group)
            .or_default()
            .push((&entry.src, &entry.dest, entry.os));
    }

    for (group, mut entries) in groups {
        entries.sort_by(|a, b| a.0.cmp(b.0));
        printer.group_header(&group);
        for (src, dest, os) in &entries {
            let src_path = std::path::Path::new(src);
            let dest_path = std::path::Path::new(dest);
            if *os == Platform::All {
                printer.arrow("entry", src_path, dest_path);
            } else {
                let label = format!("entry [{os}]");
                printer.arrow(&label, src_path, dest_path);
            }
        }
    }

    printer.annotation(&format!("\n{} tracked entries", config.entries.len()));
    Ok(())
}
