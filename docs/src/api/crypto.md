# crypto

ChaCha20-Poly1305 encryption with Argon2id key derivation, and vault management.

## `crypto`

### `encrypt_with_key()`

```rust
pub fn encrypt_with_key(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>>
```

Encrypts data using ChaCha20-Poly1305 with the given 32-byte key. Generates a random 12-byte nonce. Returns the encrypted data in the `DOTLING-ENC-V2` format:

```
DOTLING-ENC-V2
<12-byte nonce as hex>
<base64-encoded ciphertext + 16-byte auth tag>
```

### `decrypt_with_key()`

```rust
pub fn decrypt_with_key(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>>
```

Decrypts data in the `DOTLING-ENC-V2` format using the given 32-byte key. Returns the plaintext. Fails if the authentication tag is invalid (data tampered or wrong key).

### `is_encrypted_content()`

```rust
pub fn is_encrypted_content(data: &[u8]) -> bool
```

Returns `true` if the data starts with the `DOTLING-ENC-V2` header.

---

## `crypto::vault`

### `vault_dir()`

```rust
pub fn vault_dir() -> Result<PathBuf>
```

Returns the path to `~/.dotling/vault/`. Creates it if it doesn't exist.

### `vault_exists()`

```rust
pub fn vault_exists() -> bool
```

Returns `true` if a vault has been initialized (both `identity.enc` and `config.toml` exist).

### `init_vault()`

```rust
pub fn init_vault(password: &str) -> Result<()>
```

Initializes a new vault. Generates a random 32-byte identity secret, derives a key from the password using Argon2id, and encrypts the identity with ChaCha20-Poly1305. Writes `identity.enc` and `config.toml` to the vault directory.

### `unlock_vault()`

```rust
pub fn unlock_vault(password: &str) -> Result<[u8; 32]>
```

Unlocks the vault by decrypting the identity secret. Returns the 32-byte master key used for file encryption. Fails if the password is incorrect (authentication tag mismatch).

### `export_vault()`

```rust
pub fn export_vault(path: &Path, password: &str) -> Result<()>
```

Exports the vault as a single encrypted bundle file. The bundle format:

```
DOTLVAUL           (8-byte magic)
0x01               (1 byte: version)
<32-byte salt>     (Argon2id salt)
<12-byte nonce>    (ChaCha20-Poly1305 nonce)
<ciphertext + tag> (encrypted payload)
```

The encrypted payload contains the vault config and identity secret.

### `import_vault()`

```rust
pub fn import_vault(path: &Path, password: &str) -> Result<()>
```

Imports a vault from an encrypted bundle. Decrypts the bundle using the password, extracts the config and identity, and writes them to the vault directory.

### `change_password()`

```rust
pub fn change_password(old_password: &str, new_password: &str) -> Result<()>
```

Changes the vault password. Decrypts the identity with the old password and re-encrypts with the new password.
