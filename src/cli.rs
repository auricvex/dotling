/// CLI argument parsing with clap.
///
/// Defines the top-level [`Cli`] struct with a global `--verbose` flag
/// and all subcommands as a [`Command`] enum using clap's derive API.
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::platform::Platform;

/// dotling — a dotfiles management CLI.
///
/// Track files under ~, store them in a git repo, and deploy them
/// via symlinks or copies across machines.
#[derive(Debug, Parser)]
#[command(name = "dotling", version, about)]
pub struct Cli {
    /// Enable verbose output (show hints and additional details).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Available dotling subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new dotling repository or clone an existing one.
    Init {
        /// Local path to create, or a git URL to clone.
        path_or_url: String,
    },

    /// Link a file or directory into the dotling repository.
    Link {
        /// Path to the file or directory to track.
        path: PathBuf,

        /// Treat the directory as a single symlink unit instead of walking
        /// its files.
        #[arg(long)]
        as_dir: bool,

        /// Deploy as a copy instead of a symlink.
        #[arg(long)]
        copy: bool,

        /// Skip the automatic git commit after linking.
        #[arg(long)]
        no_commit: bool,

        /// Target OS for this entry (all, linux, macos, windows).
        #[arg(long, default_value = "all")]
        os: Platform,
    },

    /// Unlink a file from the dotling repository.
    Unlink {
        /// Path to the tracked file to unlink.
        path: PathBuf,

        /// Also delete the source file from the repo.
        #[arg(long)]
        purge: bool,
    },

    /// Sync dotfiles with the remote repository.
    Sync {
        /// Push local changes before pulling.
        #[arg(long)]
        push: bool,

        /// Force-overwrite modified copies during re-apply.
        #[arg(long)]
        force: bool,

        /// Show what would change without making modifications.
        #[arg(long)]
        dry_run: bool,
    },

    /// Stage, commit, and push all changes to the remote.
    Push {
        /// Commit message (defaults to "dotling: update dotfiles").
        message: Option<String>,
    },

    /// Show the deployment status of all tracked entries.
    Status,

    /// Show a diff between the repo source and the deployed file.
    Diff {
        /// Specific file to diff (diffs all modified entries if omitted).
        file: Option<PathBuf>,
    },

    /// Re-deploy missing and broken entries.
    Apply {
        /// Show what would change without making modifications.
        #[arg(long)]
        dry_run: bool,
    },

    /// Pull back a deployed copy into the repo.
    PullBack {
        /// File to pull back (filename or full destination path).
        file: String,
    },

    /// List all tracked entries, grouped by category.
    List,

    /// Audit repository health and report issues.
    Doctor,
}
