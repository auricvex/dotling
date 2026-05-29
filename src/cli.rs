use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "dotling",
    version,
    about = "A dotfiles management CLI — track, link, and sync your config files across machines",
    long_about = None,
    arg_required_else_help = true,
)]
pub struct Cli {
    /// Show verbose output with hints.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a new dotfiles repo or adopt an existing one.
    Init {
        /// Path to create the repo at, or a git URL to clone.
        #[arg(default_value = "~/dotfiles")]
        path: String,
    },

    /// Add files or directories to tracking.
    Add {
        /// Paths to add (files or directories).
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Encrypt the file(s) using the vault password.
        #[arg(long)]
        encrypt: bool,

        /// Deploy as a copy instead of a symlink.
        #[arg(long)]
        copy: bool,

        /// Track as a template (.dtmpl): rendered on each sync with
        /// machine-local variables from `~/.dotling/vars.toml`.
        #[arg(long)]
        template: bool,

        /// Restrict to a specific OS (linux, macos, windows).
        #[arg(long)]
        os: Option<String>,
    },

    /// Remove entries from tracking.
    Remove {
        /// Source paths or target paths of entries to remove.
        #[arg(required = true)]
        entries: Vec<String>,
    },

    /// Synchronise tracked entries between the repo and the actual filesystem.
    ///
    /// Pushes (repo → actual) entries that are missing or outdated,
    /// and pulls (actual → repo) copy-mode entries that were modified locally.
    /// When both sides differ, the user is prompted for resolution.
    Sync {
        /// Show what would change without modifying anything.
        #[arg(long)]
        dry_run: bool,

        /// Overwrite conflicting files without prompting (repo wins; local files
        /// are backed up automatically).
        #[arg(long)]
        force: bool,

        /// When both sides differ, prefer the actual (local) file over the repo
        /// without prompting.  Equivalent to always answering [k]eep-local.
        #[arg(long, alias = "prefer-local")]
        prefer_actual: bool,

        /// Do not prompt for conflict resolution; skip conflicting entries and
        /// print a warning.  Useful in non-interactive environments (CI, scripts).
        #[arg(long)]
        no_interactive: bool,

        /// Always back up the local file before any push that would overwrite it,
        /// even when there is no conflict.
        #[arg(long)]
        backup: bool,

        /// Allow executing all hooks without prompting.
        #[arg(long)]
        allow_hooks: bool,

        /// Disable executing any hooks.
        #[arg(long)]
        no_hooks: bool,
    },

    /// Show status of all tracked entries.
    Status {
        /// Show inline diffs for modified entries.
        #[arg(long)]
        diff: bool,
    },

    /// Edit a tracked entry in your $EDITOR.
    ///
    /// For encrypted entries, dotling decrypts to a secure temp file, opens
    /// your editor, then re-encrypts and writes back to the repo automatically.
    /// For plain or template entries, the repo source file is opened directly.
    Edit {
        /// Source path, target path, or partial match of the entry to edit.
        entry: String,
    },

    /// Encrypt or decrypt tracked entries.
    Encrypt {
        /// Paths to encrypt.
        #[arg(required = true)]
        paths: Vec<String>,
    },

    /// Decrypt encrypted entries back to plaintext in the repo.
    Decrypt {
        /// Paths to decrypt.
        #[arg(required = true)]
        paths: Vec<String>,
    },

    /// Manage the encryption vault.
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },

    /// Audit repository health and report issues.
    Doctor,

    /// Manage machine-local template variables.
    ///
    /// Variables are stored in `~/.dotling/vars.toml` and are never
    /// committed to git. Shared defaults live in `dotling.toml [vars]`.
    Vars {
        #[command(subcommand)]
        action: VarsAction,
    },

    /// Manage local file backups created by dotling before overwriting.
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },

    /// Generate shell completion scripts.
    #[command(
        long_about = "Generate shell completion scripts for the given shell.\n\n\
            The script is written to stdout. Redirect it to your shell's\n\
            completion directory to activate.\n\n\
            Examples:\n\
              dotling completions bash > ~/.local/share/bash-completion/completions/dotling\n\
              dotling completions zsh > ~/.zfunc/_dotling\n\
              dotling completions fish > ~/.config/fish/completions/dotling.fish"
    )]
    Completions {
        /// The shell to generate completions for.
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub enum VaultAction {
    /// Initialize a new vault with a password.
    Init,

    /// Show vault status and public info.
    Show,

    /// Export vault as a portable encrypted bundle.
    Export {
        /// Path to write the vault bundle.
        path: PathBuf,
    },

    /// Import a vault bundle.
    Import {
        /// Path to the vault bundle to import.
        path: PathBuf,
    },

    /// Change the vault password.
    #[command(name = "change-password")]
    ChangePassword,
}

#[derive(Subcommand)]
pub enum BackupAction {
    /// List all backup sessions.
    List,

    /// Remove old backup sessions.
    ///
    /// By default keeps the 10 most recent sessions.
    /// At least one of --keep-last or --older-than must be supplied,
    /// or the default of --keep-last 10 is used.
    Clean {
        /// Keep only the N most recent backup sessions.
        #[arg(long, value_name = "N")]
        keep_last: Option<usize>,

        /// Delete backup sessions older than D days.
        #[arg(long, value_name = "DAYS")]
        older_than: Option<u64>,
    },
}

#[derive(Subcommand)]
pub enum VarsAction {
    /// Show all resolved variables (built-in, config defaults, and local).
    List,

    /// Set a machine-local variable in `~/.dotling/vars.toml`.
    Set {
        /// Variable key.
        key: String,
        /// Variable value.
        value: String,
    },

    /// Print the resolved value of a single variable.
    Get {
        /// Variable key.
        key: String,
    },

    /// Remove a variable from the local store.
    Unset {
        /// Variable key.
        key: String,
    },

    /// Validate all template entries — find unresolved variables.
    Check,

    /// Bulk-import variables from a TOML or .env file.
    Import {
        /// Path to a TOML file with a `[vars]` section, or a `.env` file.
        path: std::path::PathBuf,
    },

    /// Print local variables as TOML (useful for migrating to a new machine).
    Export,
}
