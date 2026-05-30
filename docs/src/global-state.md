# Global State

dotling stores all state under `~/.dotling/`. This directory is never committed to git — it's machine-local.

## Directory layout

```
~/.dotling/
  state.toml              Registered repo root path
  vars.toml               Machine-local template variables
  fingerprints.toml       Sync fingerprint store (Blake2s-256 hashes)
  vault/
    identity.enc          Encrypted vault secret
    config.toml           Vault metadata
  snapshots/
    <source>              Plaintext snapshots for three-way merge base
  state/
    trusted_hooks         Blake2s-256 hashes of trusted hook commands
  tmp/                    Secure temp files for encrypted file editing
```

## File details

### `state.toml`

Stores the path to the dotfiles repository:

```toml
repo = "/home/user/dotfiles"
```

### `vars.toml`

Machine-local template variables. These override `[vars]` defaults in `dotling.toml` and are never committed to git:

```toml
[vars]
hostname = "work-laptop"
primary_user = "alice"
```

Managed via `dotling vars set/get/unset/list`.

### `fingerprints.toml`

Sync fingerprint store for change detection. Uses Blake2s-256 content hashes so dotling can check sync state without decrypting files:

```toml
[[entries]]
source = "ssh/config"
enc_hash = "a1b2c3..."
target_hash = "d4e5f6..."
source_hash = "g7h8i9..."
```

### `vault/identity.enc`

The vault's encrypted identity secret. Format:

```
DOTLING-VAULT-V1
<32-byte salt as hex>
<12-byte nonce as hex>
<base64-encoded ciphertext + auth tag>
```

### `vault/config.toml`

Vault metadata:

```toml
[vault]
version = 1
created = "2024-01-15T10:30:00Z"
```

### `snapshots/<source>`

Plaintext snapshots of copy-mode entries used as the merge base for three-way merge. One file per tracked entry, named by the source path.

### `state/trusted_hooks`

Blake2s-256 hashes of hook commands that the user has approved. One hash per line. When a hook is encountered, its hash is checked against this file to determine if it can run without prompting.

### `tmp/`

Secure temporary files created during `dotling edit` for encrypted entries. Files are created with mode `0600` and securely wiped (overwritten with zeros) before deletion.
