# dotling add

Add files or directories to tracking.

## Usage

```sh
dotling add <PATHS> [OPTIONS]
```

## Arguments

| Argument | Description |
|---|---|
| `<PATHS>` | One or more file or directory paths to track |

## Options

| Flag | Description |
|---|---|
| `--encrypt` | Encrypt the file(s) using the vault password |
| `--copy` | Deploy as a copy instead of a symlink |
| `--template` | Track as a template: rendered on each sync with machine-local variables |
| `--os <platform>` | Restrict to a specific OS: `linux`, `macos`, `windows` |

## Description

`dotling add` moves files from their original location into the repo and deploys a symlink (or copy) back. The file is recorded in `dotling.toml` with its source (repo-relative) and target (original) paths.

### Automatic path mapping

Files are organized into categories in the repo. See [Path Mapping](../configuration.md#path-mapping) for the full mapping rules.

### Directories

When adding a directory, dotling recursively moves all files and creates the corresponding entries. Each file in the directory becomes a separate entry in `dotling.toml`.

### Encryption

With `--encrypt`, the file is encrypted using the vault's master key before being stored in the repo. The encrypted file gets an `.enc` suffix. You'll be prompted for your vault password if the vault is locked.

### Templates

With `--template`, the source file is renamed with a `.dtmpl` suffix in the repo. On each `sync`, dotling renders the template with variables and writes the output to the target. See [Templates](../templates.md) for syntax details.

### OS restriction

With `--os`, the entry is tagged for a specific platform and will only be deployed when the current OS matches.

## Examples

```sh
# Track a single file
dotling add ~/.zshrc

# Track a directory
dotling add ~/.config/nvim

# Track with encryption
dotling add ~/.ssh/config --encrypt

# Track as a copy (not symlink)
dotling add ~/.config/some-app --copy

# Track as a template
dotling add ~/.config/nix-darwin/flake.nix --template

# Track for a specific OS
dotling add ~/.bashrc --os linux

# Combine flags
dotling add ~/.config/secret.conf --template --encrypt --os macos
```
