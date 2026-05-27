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

        /// Restrict to a specific OS (linux, macos, windows).
        #[arg(long)]
        os: Option<String>,
    },

    /// Remove entries from tracking.
    Remove {
        /// Source paths or target paths of entries to remove.
        #[arg(required = true)]
        entries: Vec<String>,

        /// Also delete the source files from the repo.
        #[arg(long)]
        purge: bool,
    },

    /// Deploy all tracked entries.
    Deploy {
        /// Show what would change without modifying anything.
        #[arg(long)]
        dry_run: bool,

        /// Overwrite conflicting files.
        #[arg(long)]
        force: bool,
    },

    /// Show status of all tracked entries.
    Status {
        /// Show inline diffs for modified entries.
        #[arg(long)]
        diff: bool,
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
