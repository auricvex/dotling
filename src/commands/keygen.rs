use std::{fmt::Write as _, fs};

use crate::{
    crypto,
    error::{DotlingError, Result, io_err},
    printer::Printer,
};

pub fn run(printer: &Printer, save: bool) -> Result<()> {
    printer.annotation("Generating age keypair...");

    let (public, secret) = crypto::generate_keypair();

    if save {
        if let Some(config_dir) = dirs::config_dir() {
            let dotling_dir = config_dir.join("dotling");
            fs::create_dir_all(&dotling_dir).map_err(io_err(&dotling_dir))?;
            let identity_file = dotling_dir.join("identity.txt");

            if identity_file.exists() {
                return Err(DotlingError::Crypto(format!(
                    "Identity file already exists at {}. Will not overwrite.",
                    identity_file.display()
                )));
            }

            let mut content = String::new();
            content.push_str("# dotling age identity file\n");
            let _ = writeln!(content, "# public key: {public}");
            content.push_str(&secret);
            content.push('\n');

            // Set secure permissions if unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut options = fs::OpenOptions::new();
                options.write(true).create(true).mode(0o600);
                let _ = options
                    .open(&identity_file)
                    .map_err(io_err(&identity_file))?;
            }

            fs::write(&identity_file, content).map_err(io_err(&identity_file))?;
            printer.success(&format!("Saved identity to {}", identity_file.display()));
            println!("  Public key (add this to .dotling.toml [encryption] recipients): {public}");
        } else {
            return Err(DotlingError::Crypto(
                "Could not determine config directory to save identity".to_string(),
            ));
        }
    } else {
        printer.success("Generated keypair:");
        println!("  # public key: {public}");
        println!("  {secret}");
        printer.annotation("Use --save to write this to your config directory automatically.");
    }

    Ok(())
}
