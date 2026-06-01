# Encryption

dotling includes a built-in portable encryption vault protected by Argon2id and ChaCha20-Poly1305. This lets you safely commit API keys, `.env` files, or ssh configs to your public dotfiles repo.

## Vault architecture

The vault lives at `~/.dotling/vault/` and contains:

| File | Purpose |
|---|---|
| `identity.enc` | Encrypted secret (your vault's master key) |
| `config.toml` | Vault metadata (creation date, version) |

### Key derivation

Your vault password is processed through **Argon2id** with a 32-byte random salt, producing a 32-byte encryption key. This key is used to encrypt a randomly generated 32-byte identity secret with **ChaCha20-Poly1305** (12-byte random nonce).

### Encrypted file format

Each encrypted file uses this format:

```
DOTLING-ENC-V2
<12-byte nonce as hex>
<base64-encoded ciphertext + 16-byte Poly1305 auth tag>
```

The ciphertext is the file content encrypted with ChaCha20-Poly1305 using the vault's master key.

## Using encryption

### 1. Initialize your vault

```sh
dotling vault init
```

You'll be prompted for a password (entered twice for confirmation).

### 2. Add a file with encryption

```sh
dotling add ~/.ssh/config --encrypt
```

dotling reads your local file, encrypts it, stores the ciphertext in your git repo, and deploys the decrypted file locally with secure permissions.

### 3. Sync encrypted entries

`dotling sync` handles encrypted entries in both directions:

```sh
# Edit your deployed file, then sync it back
vim ~/.ssh/config
dotling sync   # detects the file is newer -> re-encrypts
```

| Direction | Trigger |
|---|---|
| Push (decrypt) | Source file newer or target missing |
| Pull (re-encrypt) | Target file newer |

### 4. Edit encrypted files

Use `dotling edit` to open an encrypted file in your `$EDITOR` without a manual decrypt/encrypt cycle:

```sh
dotling edit ~/.ssh/config
dotling edit ssh/config       # also works with repo paths
```

The decrypted content is written to a secure temp file in `~/.dotling/tmp/` with mode `0600`. Temp files are securely wiped (overwritten with zeros) before deletion.

**Editor resolution:** `$DOTLING_EDITOR` -> `$VISUAL` -> `$EDITOR` -> `vim` -> `nano` -> `vi`. GUI editors (`code`, `subl`, `zed`, `pulsar`, `atom`) automatically get `--wait` appended.

### 5. Encrypt and decrypt in-place

```sh
dotling encrypt <paths>    # encrypt tracked entries
dotling decrypt <paths>    # decrypt back to plaintext
```

These modify files in-place in the repo.

## Migrating to a new machine

### Export

From your old machine, export the vault as a single encrypted bundle:

```sh
dotling vault export my-vault.bundle
```

### Import

On the new machine, import the bundle and sync:

```sh
dotling vault import my-vault.bundle
dotling sync
```

You'll be prompted for your vault password during import.

### Bundle format

The vault bundle is a single encrypted file:

```
DOTLVAUL           (8-byte magic)
0x01               (1 byte: version)
<32-byte salt>     (Argon2id salt)
<12-byte nonce>    (ChaCha20-Poly1305 nonce)
<ciphertext + tag> (encrypted payload)
```

The encrypted payload contains the vault config and identity secret.

## Other vault commands

```sh
dotling vault init              # create a new vault
dotling vault show              # display vault status and location
dotling vault export <path>     # export as encrypted bundle
dotling vault import <path>     # import a bundle
dotling vault change-password   # change the vault password
```

## Sync fingerprints

Previously, encrypted entries had to be decrypted to verify their sync state. dotling uses lightweight Blake2s-256 sync fingerprints stored in `~/.dotling/fingerprints.toml`:

- After each successful sync, dotling records the content hashes of the encrypted ciphertext and the local plaintext target
- On subsequent `status` or `sync` checks, dotling compares current file hashes against the stored fingerprint
- **Benefit:** You can run `dotling status` or `dotling sync --dry-run` without entering your vault password. A password is only requested when actual file modifications need to be decrypted or re-encrypted.
