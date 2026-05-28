use std::{
    collections::HashSet,
    fmt::Write as _,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use blake2::{Blake2s256, Digest};

use crate::{
    config::Entry,
    error::{Error, Result},
    ui,
};

/// Tracks trust state and manages lifecycle hook execution during a dotling session.
pub struct HookSession {
    trusted_hashes: HashSet<String>,
    trust_store_path: PathBuf,
    allow_hooks: bool,
    no_hooks: bool,
    skip_all: bool,
}

impl HookSession {
    /// Create a new hook session.
    ///
    /// Respects the CLI flags as well as environment variables:
    /// - `DOTLING_ALLOW_HOOKS=1` automatically trusts all hooks without prompting.
    /// - `DOTLING_NO_HOOKS=1` completely disables executing any hooks.
    pub fn new(mut allow_hooks: bool, mut no_hooks: bool) -> Self {
        if !allow_hooks
            && std::env::var("DOTLING_ALLOW_HOOKS")
                .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        {
            allow_hooks = true;
        }
        if !no_hooks
            && std::env::var("DOTLING_NO_HOOKS")
                .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        {
            no_hooks = true;
        }

        let trust_store_path = match crate::store::state_dir() {
            Ok(dir) => dir.join("trusted_hooks"),
            Err(_) => PathBuf::from(".dotling_trusted_hooks"),
        };

        let mut trusted_hashes = HashSet::new();
        if trust_store_path.exists() {
            if let Ok(content) = fs::read_to_string(&trust_store_path) {
                for line in content.lines() {
                    let hash = line.trim();
                    if !hash.is_empty() {
                        trusted_hashes.insert(hash.to_string());
                    }
                }
            }
        }

        Self {
            trusted_hashes,
            trust_store_path,
            allow_hooks,
            no_hooks,
            skip_all: false,
        }
    }

    /// Add a hook's hash to the trusted store.
    fn trust_hook(&mut self, hash: &str) -> Result<()> {
        self.trusted_hashes.insert(hash.to_string());
        if let Some(parent) = self.trust_store_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::io(parent, "create trust store directory", e))?;
        }

        let mut content = String::new();
        for h in &self.trusted_hashes {
            let _ = writeln!(content, "{h}");
        }
        crate::fs::atomic_write(&self.trust_store_path, content.as_bytes())?;
        Ok(())
    }

    /// Prompt the user to trust and execute an untrusted hook.
    pub fn verify_and_allow(
        &mut self,
        command: &str,
        hook_type: &str,
        no_interactive: bool,
    ) -> Result<bool> {
        if self.no_hooks || self.skip_all {
            return Ok(false);
        }

        // Calculate Blake2s-256 hash of the command string
        let mut hasher = Blake2s256::new();
        hasher.update(command.as_bytes());
        let hash = hex_encode(&hasher.finalize());

        if self.allow_hooks || self.trusted_hashes.contains(&hash) {
            return Ok(true);
        }

        if no_interactive {
            ui::warning(&format!(
                "Skipping untrusted {hook_type} hook (non-interactive): '{command}'"
            ));
            return Ok(false);
        }

        println!(
            "\n  {} Untrusted hook detected (type: {}):",
            ui::paint(ui::MAGENTA, "⚡"),
            ui::paint(ui::BOLD, hook_type)
        );
        println!("    {}", ui::paint(ui::CYAN, command));

        loop {
            print!(
                "    {} Do you want to run this hook? [y]es (once) / [n]o (skip) / [a]lways (trust) / [s]kip all > ",
                ui::paint(ui::YELLOW, "?"),
            );
            io::stdout().flush().ok();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() || input.is_empty() {
                return Ok(false);
            }

            match input.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                "a" | "always" => {
                    self.trust_hook(&hash)?;
                    ui::success("Hook trusted and saved.");
                    return Ok(true);
                }
                "s" | "skip-all" | "skipall" => {
                    self.skip_all = true;
                    return Ok(false);
                }
                _ => {
                    println!(
                        "    {}",
                        ui::paint(ui::DIM, "unrecognised — type y, n, a, or s")
                    );
                }
            }
        }
    }

    /// Execute a hook command, with rich process environment variables, streaming outputs,
    /// and running in the repository root folder.
    #[allow(clippy::too_many_arguments)]
    pub fn run_hook(
        &mut self,
        command: &str,
        hook_type: &str,
        repo_root: &Path,
        dry_run: bool,
        no_interactive: bool,
        entry: Option<&Entry>,
        entry_action: Option<&str>,
    ) -> Result<()> {
        if self.no_hooks {
            return Ok(());
        }

        if dry_run {
            let label = if let Some(e) = entry {
                format!("entry '{}' {hook_type}", e.source)
            } else {
                format!("global {hook_type}")
            };
            ui::info(&format!(
                "would run {label} hook: '{}'",
                ui::paint(ui::CYAN, command)
            ));
            return Ok(());
        }

        if !self.verify_and_allow(command, hook_type, no_interactive)? {
            return Ok(());
        }

        let label = if let Some(e) = entry {
            format!("entry '{}' {hook_type}", e.source)
        } else {
            format!("global {hook_type}")
        };
        ui::info(&format!(
            "Running {label} hook: '{}'",
            ui::paint(ui::CYAN, command)
        ));

        // Command execution using standard shell
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C").arg(command);
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(command);
            c
        };

        // Execution working directory and standard I/O inheritance
        cmd.current_dir(repo_root);
        cmd.stdout(std::process::Stdio::inherit());
        cmd.stderr(std::process::Stdio::inherit());

        // Context environment variables
        cmd.env("DOTLING_HOOK_TYPE", hook_type);
        cmd.env("DOTLING_REPO_ROOT", repo_root.to_string_lossy().as_ref());
        cmd.env("DOTLING_DRY_RUN", if dry_run { "true" } else { "false" });

        if let Some(e) = entry {
            cmd.env("DOTLING_ENTRY_SOURCE", &e.source);
            cmd.env("DOTLING_ENTRY_TARGET", &e.target);
            if let Some(action) = entry_action {
                cmd.env("DOTLING_ENTRY_ACTION", action);
            }
        }

        let status = cmd
            .status()
            .map_err(|e| Error::User(format!("failed to start hook command '{command}': {e}")))?;

        if !status.success() {
            return Err(Error::User(format!(
                "hook command '{command}' failed with {status}"
            )));
        }

        Ok(())
    }
}

fn hex_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        let _ = write!(out, "{b:02x}");
    }
    out
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(b"hello"), "68656c6c6f");
    }

    #[test]
    fn test_hook_session_trust_store() {
        let temp = tempdir().unwrap();
        let store_path = temp.path().join("trusted_hooks");

        let mut session = HookSession {
            trusted_hashes: HashSet::new(),
            trust_store_path: store_path.clone(),
            allow_hooks: false,
            no_hooks: false,
            skip_all: false,
        };

        // Hash of "echo test"
        let mut hasher = Blake2s256::new();
        hasher.update(b"echo test");
        let hash = hex_encode(&hasher.finalize());

        assert!(!session.trusted_hashes.contains(&hash));
        session.trust_hook(&hash).unwrap();
        assert!(session.trusted_hashes.contains(&hash));
        assert!(store_path.exists());

        // Reload
        let content = fs::read_to_string(&store_path).unwrap();
        assert!(content.contains(&hash));
    }

    #[test]
    fn test_run_hook_dry_run() {
        let temp = tempdir().unwrap();
        let mut session = HookSession::new(false, false);

        // Dry-run should succeed and not run anything
        session
            .run_hook("exit 1", "test", temp.path(), true, true, None, None)
            .unwrap();
    }

    #[test]
    fn test_run_hook_allow_hooks() {
        let temp = tempdir().unwrap();
        let mut session = HookSession::new(true, false);

        // Should execute command successfully without prompt because allow_hooks is true
        session
            .run_hook(
                "echo 'hello world'",
                "test",
                temp.path(),
                false,
                true,
                None,
                None,
            )
            .unwrap();
    }

    #[test]
    fn test_run_hook_no_hooks() {
        let temp = tempdir().unwrap();
        let mut session = HookSession::new(false, true);

        // Should return early and not run anything or prompt
        session
            .run_hook("exit 1", "test", temp.path(), false, true, None, None)
            .unwrap();
    }
}
