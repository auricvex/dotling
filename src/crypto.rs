use std::{
    io::{Read, Write},
    str::FromStr,
};

use age::x25519::{Identity, Recipient};
use secrecy::ExposeSecret;

use crate::error::{DotlingError, Result};

/// Encrypts plaintext using the given age recipients.
pub fn encrypt(plaintext: &[u8], recipient_strings: &[String]) -> Result<Vec<u8>> {
    let mut recipients: Vec<Box<dyn age::Recipient + Send>> = Vec::new();
    for r in recipient_strings {
        let recipient = Recipient::from_str(r)
            .map_err(|e| DotlingError::Crypto(format!("Invalid recipient '{r}': {e}")))?;
        recipients.push(Box::new(recipient));
    }

    if recipients.is_empty() {
        return Err(DotlingError::Crypto(
            "No encryption recipients configured in .dotling.toml".to_string(),
        ));
    }

    let encryptor = age::Encryptor::with_recipients(
        recipients.iter().map(|r| r.as_ref() as &dyn age::Recipient),
    )
    .map_err(|e| DotlingError::Crypto(format!("Failed to create encryptor: {e}")))?;

    let mut encrypted = Vec::new();
    {
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .map_err(|e| DotlingError::Crypto(format!("Encryption failed: {e}")))?;
        writer
            .write_all(plaintext)
            .map_err(|e| DotlingError::Crypto(format!("Failed to write encrypted data: {e}")))?;
        writer
            .finish()
            .map_err(|e| DotlingError::Crypto(format!("Failed to finish encryption: {e}")))?;
    }

    Ok(encrypted)
}

/// Decrypts ciphertext using the provided identity string.
pub fn decrypt(ciphertext: &[u8], identity_string: &str) -> Result<Vec<u8>> {
    let identity = Identity::from_str(identity_string)
        .map_err(|e| DotlingError::Crypto(format!("Invalid identity key: {e}")))?;

    let decryptor = age::Decryptor::new(ciphertext)
        .map_err(|e| DotlingError::Crypto(format!("Decryption failed: {e}")))?;

    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| DotlingError::Crypto(format!("Failed to decrypt: {e}")))?;

    let mut plaintext = Vec::new();
    reader
        .read_to_end(&mut plaintext)
        .map_err(|e| DotlingError::Crypto(format!("Failed to read plaintext: {e}")))?;

    Ok(plaintext)
}

/// Generates a new age x25519 keypair, returning (`public_key`, `secret_key`).
pub fn generate_keypair() -> (String, String) {
    let identity = Identity::generate();
    let public = identity.to_public().to_string();
    let secret = identity.to_string().expose_secret().to_string();
    (public, secret)
}

/// Retrieves the default identity string from `~/.config/dotling/identity.txt`.
pub fn get_default_identity() -> Result<String> {
    if let Some(config_dir) = dirs::config_dir() {
        let identity_file = config_dir.join("dotling").join("identity.txt");
        if identity_file.exists() {
            let content = std::fs::read_to_string(&identity_file)
                .map_err(crate::error::io_err(&identity_file))?;
            return Ok(content);
        }
    }
    Err(DotlingError::Crypto(
        "No identity file found at ~/.config/dotling/identity.txt. Run `dotling keygen --save` first.".to_string()
    ))
}
