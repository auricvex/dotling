//! Integration tests for the encrypt/decrypt roundtrip.
//!
//! These tests exercise the full encrypt→decrypt cycle on a real (temp) repo
//! structure to catch bugs like template deletion during decryption.

use std::fs;

use dotling::{
    commands::encrypt::{decrypt_single_entry, encrypt_single_entry},
    config::Entry,
};

fn test_key() -> [u8; 32] {
    [0x42u8; 32]
}

fn make_entry(source: &str, target: &str, template: bool, encrypted: bool) -> Entry {
    Entry {
        source: source.into(),
        target: target.into(),
        method: None,
        encrypted,
        directory: false,
        template,
        os: None,
        permissions: None,
        before: None,
        after: None,
    }
}

fn make_dir_entry(source: &str, target: &str, encrypted: bool) -> Entry {
    Entry {
        source: source.into(),
        target: target.into(),
        method: None,
        encrypted,
        directory: true,
        template: false,
        os: None,
        permissions: None,
        before: None,
        after: None,
    }
}

/// Template encrypt→decrypt roundtrip: files stay in-place.
#[test]
fn template_encrypt_decrypt_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join("shell")).unwrap();

    let key = test_key();
    let original = b"# zshrc template\nexport EDITOR={{ var.editor | default \"vim\" }}\n";

    // Write the plaintext template
    fs::write(repo.join("shell/zshrc.dtmpl"), original).unwrap();

    // Encrypt
    let mut entry = make_entry("shell/zshrc.dtmpl", "~/.zshrc", true, false);
    encrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(entry.encrypted);

    // After encrypt: file stays in-place with encrypted content
    let enc = fs::read(repo.join("shell/zshrc.dtmpl")).unwrap();
    assert!(dotling::crypto::is_encrypted_content(&enc));
    let dec = dotling::crypto::decrypt_with_key(&enc, &key).unwrap();
    assert_eq!(dec, original);

    // Decrypt
    decrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(!entry.encrypted);

    // After decrypt: file is back to plaintext in-place
    let content = fs::read(repo.join("shell/zshrc.dtmpl")).unwrap();
    assert_eq!(
        content, original,
        "decrypted content must match original template"
    );
}

/// Template encrypt→decrypt with source already containing .enc.
#[test]
fn template_encrypt_decrypt_with_enc_in_source() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join("shell")).unwrap();

    let key = test_key();
    let original = b"{{ var.name }}";

    // Encrypted content stored at the source path in-place
    let source_path = repo.join("shell/zshrc.dtmpl.enc");
    let encrypted = dotling::crypto::encrypt_with_key(original, &key).unwrap();
    dotling::fs::atomic_write(&source_path, &encrypted).unwrap();

    let mut entry = make_entry("shell/zshrc.dtmpl.enc", "~/.zshrc", true, true);

    // Decrypt
    decrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(!entry.encrypted);

    // Decrypted in-place
    assert_eq!(fs::read(&source_path).unwrap(), original);
}

/// Plain file encrypt→decrypt roundtrip: files stay in-place.
#[test]
fn plain_file_encrypt_decrypt_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join("shell")).unwrap();

    let key = test_key();
    let original = b"# .zshrc\nexport PATH=$HOME/bin:$PATH\nalias ll='ls -la'\n";
    fs::write(repo.join("shell/zshrc"), original).unwrap();

    let mut entry = make_entry("shell/zshrc", "~/.zshrc", false, false);

    // Encrypt
    encrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(entry.encrypted);
    // File stays in-place with encrypted content
    let enc = fs::read(repo.join("shell/zshrc")).unwrap();
    assert!(dotling::crypto::is_encrypted_content(&enc));

    // Decrypt
    decrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(!entry.encrypted);
    assert_eq!(fs::read(repo.join("shell/zshrc")).unwrap(), original);
}

/// Directory encrypt→decrypt roundtrip: files stay in-place.
#[test]
fn directory_encrypt_decrypt_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let dir = repo.join("secrets");
    fs::create_dir_all(dir.join("sub")).unwrap();

    let key = test_key();
    fs::write(dir.join("id_rsa"), b"private-key-data").unwrap();
    fs::write(dir.join("id_rsa.pub"), b"public-key-data").unwrap();
    fs::write(dir.join("sub/config"), b"ssh config").unwrap();

    let mut entry = make_dir_entry("secrets", "~/.ssh", false);

    // Encrypt
    encrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(entry.encrypted);
    // Files stay in-place with encrypted content
    assert!(dotling::crypto::is_encrypted_content(
        &fs::read(dir.join("id_rsa")).unwrap()
    ));
    assert!(dotling::crypto::is_encrypted_content(
        &fs::read(dir.join("id_rsa.pub")).unwrap()
    ));
    assert!(dotling::crypto::is_encrypted_content(
        &fs::read(dir.join("sub/config")).unwrap()
    ));

    // Decrypt
    decrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert!(!entry.encrypted);
    assert_eq!(fs::read(dir.join("id_rsa")).unwrap(), b"private-key-data");
    assert_eq!(
        fs::read(dir.join("id_rsa.pub")).unwrap(),
        b"public-key-data"
    );
    assert_eq!(fs::read(dir.join("sub/config")).unwrap(), b"ssh config");
}

/// Double encrypt→decrypt roundtrip (idempotency check).
#[test]
fn double_roundtrip_preserves_content() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join("git")).unwrap();

    let key = test_key();
    let original = b"[user]\n  name = Test\n  email = test@example.com\n";
    fs::write(repo.join("git/gitconfig"), original).unwrap();

    let mut entry = make_entry("git/gitconfig", "~/.gitconfig", false, false);

    // First roundtrip
    encrypt_single_entry(&mut entry, &repo, &key).unwrap();
    decrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert_eq!(fs::read(repo.join("git/gitconfig")).unwrap(), original);

    // Second roundtrip
    encrypt_single_entry(&mut entry, &repo, &key).unwrap();
    decrypt_single_entry(&mut entry, &repo, &key).unwrap();
    assert_eq!(fs::read(repo.join("git/gitconfig")).unwrap(), original);
}
