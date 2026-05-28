use std::path::Path;

use crate::{
    config::Config,
    error::{Error, Result},
    store,
    template::RenderContext,
    ui,
    vars::{self as var_store, VarStore},
};

// ── dotling vars list ──────────────────────────────────────────────

/// Show all resolved variables with their sources.
pub fn run_list() -> Result<()> {
    let var_store = VarStore::load()?;
    let config_vars = load_config_vars();

    let repo_root = store::get_repo_root()?
        .map_or_else(|| "(no repo)".into(), |p| p.to_string_lossy().into_owned());

    let ctx = RenderContext::new(&repo_root, &config_vars, &var_store.as_pairs());

    let hostname = ctx
        .builtins
        .get("hostname")
        .cloned()
        .unwrap_or_else(|| "unknown".into());

    println!();
    println!(
        "  {}",
        ui::paint(
            ui::BOLD,
            &format!("Variable resolution \u{2014} {hostname}")
        )
    );
    let divider = "\u{2500}".repeat(54);
    println!("  {}", ui::paint(ui::DIM, &divider));

    for key in &["hostname", "username", "os", "arch", "home", "repo"] {
        if let Some(val) = ctx.builtins.get(*key) {
            println!(
                "  {:<28} {}  {}",
                ui::paint(ui::CYAN, &format!("dotling.{key}")),
                val,
                ui::paint(ui::DIM, "[auto]")
            );
        }
    }

    println!("  {}", ui::paint(ui::DIM, &divider));

    let mut shown: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for (key, val) in var_store.iter() {
        shown.insert(key.to_string());
        println!(
            "  {:<28} {}  {}",
            ui::paint(ui::CYAN, &format!("var.{key}")),
            val,
            ui::paint(ui::DIM, "[local]")
        );
    }

    for (key, val) in &config_vars {
        if shown.contains(key) {
            continue;
        }
        shown.insert(key.clone());
        println!(
            "  {:<28} {}  {}",
            ui::paint(ui::CYAN, &format!("var.{key}")),
            val,
            ui::paint(ui::DIM, "[default]")
        );
    }

    if shown.is_empty() {
        ui::hint("no variables defined \u{2014} use `dotling vars set <key> <value>`");
    }

    println!();
    Ok(())
}

// ── dotling vars set ───────────────────────────────────────────────

pub fn run_set(key: &str, value: &str) -> Result<()> {
    let mut store = VarStore::load()?;
    let is_new = store.get(key).is_none();
    store.set(key, value);
    store.save()?;

    if is_new {
        ui::success(&format!(
            "set {key} = \"{value}\"  (saved to ~/.dotling/vars.toml)"
        ));
    } else {
        ui::success(&format!("updated {key} = \"{value}\""));
    }
    Ok(())
}

// ── dotling vars get ───────────────────────────────────────────────

pub fn run_get(key: &str) -> Result<()> {
    let repo_root =
        store::get_repo_root()?.map_or_else(String::new, |p| p.to_string_lossy().into_owned());
    let config_vars = load_config_vars();
    let var_store = VarStore::load()?;
    let ctx = RenderContext::new(&repo_root, &config_vars, &var_store.as_pairs());

    if let Some(val) = ctx.builtins.get(key) {
        println!(
            "  {} {}",
            ui::paint(ui::CYAN, &format!("dotling.{key}")),
            val
        );
        return Ok(());
    }

    if let Some(val) = ctx.resolve("var", key) {
        let source = if var_store.get(key).is_some() {
            "[local]"
        } else {
            "[default]"
        };
        println!(
            "  {} {}  {}",
            ui::paint(ui::CYAN, &format!("var.{key}")),
            val,
            ui::paint(ui::DIM, source)
        );
        return Ok(());
    }

    Err(Error::User(format!("variable `{key}` is not set")))
}

// ── dotling vars unset ─────────────────────────────────────────────

pub fn run_unset(key: &str) -> Result<()> {
    let mut store = VarStore::load()?;
    if store.remove(key) {
        store.save()?;
        ui::success(&format!("unset {key}"));
    } else {
        ui::warning(&format!("`{key}` was not set in the local store"));
    }
    Ok(())
}

// ── dotling vars check ─────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub fn run_check() -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let config = Config::load(&config_path)?;

    let template_entries: Vec<_> = config.entries.iter().filter(|e| e.template).collect();

    if template_entries.is_empty() {
        ui::info("no template entries found");
        ui::hint("add a template with `dotling add <path> --template`");
        return Ok(());
    }

    let var_store = VarStore::load()?;
    let repo_root_str = repo_root.to_string_lossy().into_owned();
    let ctx = RenderContext::new(&repo_root_str, &config.vars, &var_store.as_pairs());

    println!();
    println!("  {}", ui::paint(ui::BOLD, "Template variable check"));
    let divider = "\u{2500}".repeat(66);
    println!("  {}", ui::paint(ui::DIM, &divider));

    let mut all_ok = true;

    for entry in &template_entries {
        let source_path = repo_root.join(&entry.source);

        if !source_path.exists() {
            println!(
                "  {} {} \u{2014} source file not found in repo",
                ui::paint(ui::RED, "\u{2717}"),
                ui::paint(ui::CYAN, &entry.source),
            );
            all_ok = false;
            continue;
        }

        if entry.encrypted {
            println!(
                "  {} {} \u{2014} encrypted template (cannot validate without vault password)",
                ui::paint(ui::YELLOW, "\u{26a0}"),
                ui::paint(ui::CYAN, &entry.source),
            );
            continue;
        }

        let template_text = std::fs::read_to_string(&source_path)
            .map_err(|e| Error::io(&source_path, "read template", e))?;

        let vars = crate::template::scan_variables(&template_text);

        if vars.is_empty() {
            println!(
                "  {} {} \u{2014} no template variables found",
                ui::paint(ui::YELLOW, "\u{26a0}"),
                ui::paint(ui::CYAN, &entry.source),
            );
            continue;
        }

        let mut missing: Vec<String> = Vec::new();
        for var in &vars {
            if var.namespace == "dotling" && !ctx.builtins.contains_key(&var.key) {
                missing.push(format!("dotling.{}", var.key));
            } else if var.namespace == "var" && ctx.resolve("var", &var.key).is_none() {
                missing.push(format!("var.{}", var.key));
            }
        }

        if missing.is_empty() {
            println!(
                "  {} {}  \u{2014}  {} variable{} \u{2014} all resolved",
                ui::paint(ui::GREEN, "\u{2713}"),
                ui::paint(ui::CYAN, &entry.source),
                vars.len(),
                if vars.len() == 1 { "" } else { "s" }
            );
        } else {
            all_ok = false;
            println!(
                "  {} {}  \u{2014}  missing: {}",
                ui::paint(ui::RED, "\u{2717}"),
                ui::paint(ui::CYAN, &entry.source),
                missing.join(", ")
            );
            for m in &missing {
                let key = m.strip_prefix("var.").unwrap_or(m);
                println!(
                    "     {} dotling vars set {} <value>",
                    ui::paint(ui::DIM, "\u{2192}"),
                    key
                );
            }
        }
    }

    println!("  {}", ui::paint(ui::DIM, &divider));

    if all_ok {
        ui::success("all templates are valid");
    } else {
        ui::warning("some templates have unresolved variables");
    }

    println!();
    Ok(())
}

// ── dotling vars import ────────────────────────────────────────────

pub fn run_import(path: &Path) -> Result<()> {
    let mut store = VarStore::load()?;
    let count = var_store::import_from_file(&mut store, path)?;
    store.save()?;
    ui::success(&format!(
        "imported {count} variable{} from {}",
        if count == 1 { "" } else { "s" },
        path.display()
    ));
    Ok(())
}

// ── dotling vars export ────────────────────────────────────────────

pub fn run_export() -> Result<()> {
    let store = VarStore::load()?;
    if store.is_empty() {
        ui::info("no local variables to export");
        return Ok(());
    }
    println!("[vars]");
    for (key, val) in store.iter() {
        let escaped = val.replace('\\', "\\\\").replace('"', "\\\"");
        println!("{key} = \"{escaped}\"");
    }
    Ok(())
}

// ── Helper ─────────────────────────────────────────────────────────

/// Load `[vars]` from dotling.toml if a repo is initialized, or return empty.
pub fn load_config_vars() -> Vec<(String, String)> {
    let Ok(Some(repo_root)) = store::get_repo_root() else {
        return Vec::new();
    };
    let config_path = store::config_path(&repo_root);
    Config::load(&config_path)
        .map(|c| c.vars)
        .unwrap_or_default()
}

// ── Bootstrap prompt ───────────────────────────────────────────────

/// Interactively collect values for unresolved template variables on a fresh
/// machine and save them to `~/.dotling/vars.toml`.
///
/// Returns `true` if any values were collected.
pub fn bootstrap_prompt(
    missing_vars: &[String],
    config_vars: &[(String, String)],
    store: &mut VarStore,
) -> bool {
    use std::io::{self, BufRead, Write};

    if missing_vars.is_empty() {
        return false;
    }

    println!();
    println!(
        "  {} Template variables needed \u{2014} first sync on this machine.",
        ui::paint(ui::YELLOW, "\u{2699}")
    );
    println!("  You can also run `dotling vars set <key> <value>` at any time.");
    println!();

    let mut any_set = false;

    for key in missing_vars {
        let default_val: Option<&str> = config_vars
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str());

        if let Some(default) = default_val {
            print!(
                "  {} {} {}\n  \u{2192} Enter value (Enter to use default \"{default}\"): ",
                ui::paint(ui::CYAN, "var"),
                ui::paint(ui::BOLD, key),
                ui::paint(ui::DIM, "\u{2014}"),
            );
        } else {
            print!(
                "  {} {} {}\n  \u{2192} Enter value: ",
                ui::paint(ui::CYAN, "var"),
                ui::paint(ui::BOLD, key),
                ui::paint(ui::DIM, "\u{2014}"),
            );
        }

        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().lock().read_line(&mut input).is_err() {
            break;
        }
        let input = input.trim();

        let value = if input.is_empty() {
            if let Some(default) = default_val {
                default.to_string()
            } else {
                continue;
            }
        } else {
            input.to_string()
        };

        store.set(key, &value);
        any_set = true;
    }

    any_set
}

// ── Collect missing var keys ───────────────────────────────────────

/// Return the list of unresolved `var.*` keys across all readable template
/// entries (deduplicated, ordered by first appearance).
pub fn collect_missing_vars(
    config: &Config,
    repo_root: &std::path::Path,
    ctx: &RenderContext,
) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();

    for entry in &config.entries {
        if !entry.template || entry.encrypted {
            continue;
        }

        let source_path = repo_root.join(&entry.source);
        if !source_path.exists() {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&source_path) else {
            continue;
        };

        for var in crate::template::scan_variables(&text) {
            if var.namespace == "var"
                && ctx.resolve("var", &var.key).is_none()
                && !missing.contains(&var.key)
            {
                missing.push(var.key);
            }
        }
    }

    missing
}
