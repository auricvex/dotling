use std::process::ExitCode;

use clap::Parser;
use dotling::cli::{BackupAction, Cli, Command, VarsAction, VaultAction};

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        dotling::ui::error(&e.to_string());
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run(cli: Cli) -> dotling::Result<()> {
    match cli.command {
        Command::Init { path } => dotling::commands::init::run(&path),

        Command::Add {
            paths,
            encrypt,
            copy,
            template,
            os,
        } => dotling::commands::add::run(&paths, encrypt, copy, template, os.as_deref()),

        Command::Remove { entries } => dotling::commands::remove::run(&entries),

        Command::Sync {
            dry_run,
            force,
            prefer_actual,
            no_interactive,
            backup,
            allow_hooks,
            no_hooks,
        } => dotling::commands::sync::run(
            dry_run,
            force,
            prefer_actual,
            no_interactive,
            backup,
            allow_hooks,
            no_hooks,
        ),

        Command::Status { diff } => dotling::commands::status::run(diff),

        Command::Edit { entry } => dotling::commands::edit::run(&entry),

        Command::Encrypt { paths } => dotling::commands::encrypt::run_encrypt(&paths),

        Command::Decrypt { paths } => dotling::commands::encrypt::run_decrypt(&paths),

        Command::Vault { action } => match action {
            VaultAction::Init => dotling::commands::vault::run_init(),
            VaultAction::Show => dotling::commands::vault::run_show(),
            VaultAction::Export { path } => dotling::commands::vault::run_export(&path),
            VaultAction::Import { path } => dotling::commands::vault::run_import(&path),
            VaultAction::ChangePassword => dotling::commands::vault::run_change_password(),
        },

        Command::Doctor => dotling::commands::doctor::run(),

        Command::Vars { action } => match action {
            VarsAction::List => dotling::commands::vars::run_list(),
            VarsAction::Set { key, value } => dotling::commands::vars::run_set(&key, &value),
            VarsAction::Get { key } => dotling::commands::vars::run_get(&key),
            VarsAction::Unset { key } => dotling::commands::vars::run_unset(&key),
            VarsAction::Check => dotling::commands::vars::run_check(),
            VarsAction::Import { path } => dotling::commands::vars::run_import(&path),
            VarsAction::Export => dotling::commands::vars::run_export(),
        },

        Command::Backup { action } => match action {
            BackupAction::List => dotling::commands::backup::run_list(),
            BackupAction::Clean {
                keep_last,
                older_than,
            } => dotling::commands::backup::run_clean(keep_last, older_than),
        },
    }
}
