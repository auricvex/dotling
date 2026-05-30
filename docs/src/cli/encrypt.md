# dotling encrypt

Encrypt tracked entries in-place in the repo.

## Usage

```sh
dotling encrypt <PATHS>
```

## Arguments

| Argument | Description |
|---|---|
| `<PATHS>` | Source paths, target paths, or partial matches of entries to encrypt |

## Description

`dotling encrypt` encrypts tracked entries using the vault's master key. The plaintext file in the repo is replaced with an encrypted `.enc` file. You'll be prompted for your vault password if the vault is locked.

### Directories

When encrypting a directory entry, each file within it is encrypted individually with its own `.enc` suffix.

## Examples

```sh
# Encrypt by source path
dotling encrypt ssh/config

# Encrypt by target path
dotling encrypt ~/.ssh/config

# Encrypt multiple entries
dotling encrypt ssh/config gnupg/gpg.conf
```
