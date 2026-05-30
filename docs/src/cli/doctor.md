# dotling doctor

Audit repository health and report issues.

## Usage

```sh
dotling doctor
```

## Description

`dotling doctor` runs a comprehensive health check on your dotfiles repository and reports any issues found. It checks:

- **Repository existence** — whether the repo root exists and is valid
- **Config validity** — whether `dotling.toml` parses correctly
- **Entry states** — broken symlinks, missing source files, missing targets
- **Template variables** — unresolved variables in template entries
- **Variable defaults** — config defaults that look like real values (not placeholders)
- **Git initialization** — whether the repo is a git repository
- **Vault initialization** — whether the vault is set up
- **Orphaned files** — files in the repo that aren't tracked by any entry in `dotling.toml`

## Examples

```sh
dotling doctor
```

Sample output:

```text
[ok]   Repo root: ~/dotfiles
[ok]   Config: dotling.toml (3 entries)
[ok]   Git: initialized
[warn] Vault: not initialized (run `dotling vault init`)
[ok]   Entry: shell/zshrc -> ~/.zshrc
[err]  Entry: config/nvim -> ~/.config/nvim (broken symlink)
[ok]   Entry: ssh/config -> ~/.ssh/config (encrypted)
```
