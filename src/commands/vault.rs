use crate::{crypto, error::Result, ui};

/// Handle vault subcommands.
pub fn run_init() -> Result<()> {
    if crypto::vault::vault_exists() {
        ui::warning("vault already exists");
        ui::hint("use `dotling vault show` to view status");
        return Ok(());
    }

    ui::info("creating new vault...");
    let password = ui::password("Choose a vault password");

    if password.is_empty() {
        return Err(crate::error::Error::User("password cannot be empty".into()));
    }

    let confirm = ui::password("Confirm password");
    if password != confirm {
        return Err(crate::error::Error::User("passwords do not match".into()));
    }

    crypto::vault::init_vault(&password)?;

    ui::success("vault created");
    ui::hint("your encrypted entries are protected by this password");
    ui::hint("use `dotling vault export <path>` to create a backup for new machines");

    Ok(())
}

pub fn run_show() -> Result<()> {
    if !crypto::vault::vault_exists() {
        ui::info("no vault found");
        ui::hint("run `dotling vault init` to create one");
        return Ok(());
    }

    let vault_dir = crypto::vault::vault_dir()?;
    ui::header("Vault");
    ui::info(&format!("location: {}", vault_dir.display()));
    ui::info("status: initialized ✓");

    Ok(())
}

pub fn run_export(path: &std::path::Path) -> Result<()> {
    if !crypto::vault::vault_exists() {
        return Err(crate::error::Error::User(
            "no vault found — run `dotling vault init` first".into(),
        ));
    }

    let password = ui::password("Vault password (to verify)");
    // Verify password by unlocking
    crypto::vault::unlock_vault(&password)?;

    crypto::vault::export_vault(path, &password)?;

    ui::success(&format!("vault exported to `{}`", path.display()));
    ui::hint("transfer this file to your new machine and run `dotling vault import <path>`");

    Ok(())
}

pub fn run_import(path: &std::path::Path) -> Result<()> {
    if crypto::vault::vault_exists() && !ui::confirm("vault already exists — overwrite?") {
        ui::info("import cancelled");
        return Ok(());
    }

    crypto::vault::import_vault(path)?;

    ui::success("vault imported");
    ui::hint("run `dotling deploy` to deploy your dotfiles");

    Ok(())
}

pub fn run_change_password() -> Result<()> {
    if !crypto::vault::vault_exists() {
        return Err(crate::error::Error::User(
            "no vault found — run `dotling vault init` first".into(),
        ));
    }

    let old_password = ui::password("Current password");
    let new_password = ui::password("New password");

    if new_password.is_empty() {
        return Err(crate::error::Error::User("password cannot be empty".into()));
    }

    let confirm = ui::password("Confirm new password");
    if new_password != confirm {
        return Err(crate::error::Error::User("passwords do not match".into()));
    }

    crypto::vault::change_password(&old_password, &new_password)?;

    ui::success("vault password changed");

    Ok(())
}
