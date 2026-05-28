mod backup;
mod cli;
mod commands;
mod config;
pub mod crypto;
mod deploy;
mod error;
mod fingerprint;
pub mod fs;
pub mod hooks;
mod merge;
pub mod path;
mod platform;
mod store;
mod ui;

use std::process::ExitCode;

use clap::Parser;
use cli::{BackupAction, Cli, Command, VaultAction};

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        ui::error(&e.to_string());
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run(cli: Cli) -> error::Result<()> {
    match cli.command {
        Command::Init { path } => commands::init::run(&path),

        Command::Add {
            paths,
            encrypt,
            copy,
            os,
        } => commands::add::run(&paths, encrypt, copy, os.as_deref()),

        Command::Remove { entries } => commands::remove::run(&entries),

        Command::Sync {
            dry_run,
            force,
            prefer_actual,
            no_interactive,
            backup,
            allow_hooks,
            no_hooks,
        } => commands::sync::run(
            dry_run,
            force,
            prefer_actual,
            no_interactive,
            backup,
            allow_hooks,
            no_hooks,
        ),

        Command::Status { diff } => commands::status::run(diff),

        Command::Encrypt { paths } => commands::encrypt::run_encrypt(&paths),

        Command::Decrypt { paths } => commands::encrypt::run_decrypt(&paths),

        Command::Vault { action } => match action {
            VaultAction::Init => commands::vault::run_init(),
            VaultAction::Show => commands::vault::run_show(),
            VaultAction::Export { path } => commands::vault::run_export(&path),
            VaultAction::Import { path } => commands::vault::run_import(&path),
            VaultAction::ChangePassword => commands::vault::run_change_password(),
        },

        Command::Doctor => commands::doctor::run(),

        Command::Backup { action } => match action {
            BackupAction::List => commands::backup::run_list(),
            BackupAction::Clean {
                keep_last,
                older_than,
            } => commands::backup::run_clean(keep_last, older_than),
        },
    }
}
