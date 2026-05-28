use crate::{backup, ui};

/// Run the `dotling backup` subcommand.
pub fn run_clean(
    keep_last: Option<usize>,
    older_than_days: Option<u64>,
) -> crate::error::Result<()> {
    if keep_last.is_none() && older_than_days.is_none() {
        // Default: keep the last 10 sessions.
        let summary = backup::clean(Some(10), None)?;
        finish(summary);
        return Ok(());
    }

    let summary = backup::clean(keep_last, older_than_days)?;
    finish(summary);
    Ok(())
}

fn finish(s: backup::CleanSummary) {
    if s.total == 0 {
        ui::info("no backup sessions found");
        return;
    }
    if s.removed == 0 {
        ui::info(&format!(
            "{} backup session(s) kept — nothing to clean",
            s.total
        ));
    } else {
        ui::success(&format!(
            "removed {} of {} backup session(s)",
            s.removed, s.total,
        ));
    }
}

/// Run `dotling backup list` — show all backup sessions.
pub fn run_list() -> crate::error::Result<()> {
    let sessions = backup::list_sessions()?;
    if sessions.is_empty() {
        ui::info("no backup sessions found");
        return Ok(());
    }
    ui::header(&format!("{} backup session(s)", sessions.len()));
    for s in &sessions {
        ui::info(&s.display().to_string());
    }
    Ok(())
}
