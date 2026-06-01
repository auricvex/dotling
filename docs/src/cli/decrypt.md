# dotling decrypt

Decrypt encrypted entries back to plaintext in the repo.

## Usage

```sh
dotling decrypt <PATHS>
```

## Arguments

| Argument | Description |
|---|---|
| `<PATHS>` | Source paths, target paths, or partial matches of entries to decrypt |

## Description

`dotling decrypt` decrypts encrypted entries and replaces the encrypted files with plaintext in the repo. You'll be prompted for your vault password if the vault is locked. The entry's `encrypted` flag is removed from `dotling.toml`.

## Examples

```sh
# Decrypt by source path
dotling decrypt ssh/config

# Decrypt by target path
dotling decrypt ~/.ssh/config

# Decrypt multiple entries
dotling decrypt ssh/config gnupg/gpg.conf
```
