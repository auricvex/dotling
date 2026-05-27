/// dotling — a dotfiles management CLI.
///
/// Parses CLI arguments, creates a [`Printer`], and dispatches to the
/// appropriate command handler. Errors are printed via the printer and
/// result in a non-zero exit code.
mod cli;
mod commands;
mod config;
mod crypto;
mod error;
mod git;
mod linker;
mod platform;
mod printer;
mod repo;

use clap::Parser;

use crate::{cli::Cli, printer::Printer};

/// Entry point for the dotling CLI.
///
/// Returns a non-zero exit code on error by setting
/// [`ExitCode`](std::process::ExitCode).
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let printer = Printer::new(cli.verbose);

    let result = match cli.command {
        cli::Command::Doctor => {
            commands::doctor::run(&printer);
            return std::process::ExitCode::SUCCESS;
        }
        cli::Command::Init { path_or_url } => commands::init::run(&printer, &path_or_url),
        cli::Command::Link {
            path,
            as_dir,
            copy,
            encrypt,
            no_commit,
            os,
        } => commands::link::run(&printer, &path, as_dir, copy, encrypt, no_commit, os),
        cli::Command::Unlink { path, purge } => commands::unlink::run(&printer, &path, purge),
        cli::Command::Sync {
            push,
            force,
            dry_run,
        } => commands::sync::run(&printer, push, force, dry_run),
        cli::Command::Push { message } => commands::push::run(&printer, message.as_deref()),
        cli::Command::Status => commands::status::run(&printer),
        cli::Command::Diff { file } => commands::diff::run(&printer, file.as_deref()),
        cli::Command::Apply { dry_run } => commands::apply::run(&printer, dry_run),
        cli::Command::PullBack { file, all } => commands::pull_back::run(&printer, file.as_deref(), all),
        cli::Command::List => commands::list::run(&printer),
        cli::Command::Keygen { save } => commands::keygen::run(&printer, save),
    };

    if let Err(e) = result {
        printer.error_msg(&e.to_string());
        return std::process::ExitCode::FAILURE;
    }

    std::process::ExitCode::SUCCESS
}
