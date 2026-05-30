# Introduction

**dotling** is a zero-dependency dotfiles management CLI written in Rust. It moves your config files into a central git repository and replaces them with symlinks (or copies). It handles the tedious parts — path mapping, conflict detection, encryption, templating, hooks, and multi-OS support — so you can set up a new machine in seconds.

## Features

- **Symlink & copy deployment** — choose per file, switch anytime
- **Bidirectional sync** — `dotling sync` pushes from repo to actual and pulls from actual to repo automatically
- **Automatic path mapping** — `~/.config/nvim` becomes `config/nvim`, `~/.zshrc` becomes `shell/zshrc`
- **Multi-OS support** — tag entries as `linux`, `macos`, or `windows`; skip irrelevant files automatically
- **Secure password vault** — encrypt sensitive files using Argon2id + ChaCha20-Poly1305
- **Encrypted sync** — sync handles encrypted entries in both directions; modified plaintext is re-encrypted back into the repo automatically
- **Portable secrets** — export your vault to easily unlock secrets on a new machine
- **Native git integration** — dotling manages the symlinks, you manage the repo with native `git` commands
- **Dotfile templating** — add machine-specific values using `{{ var.key }}` syntax; render on every sync
- **Health checks** — `dotling doctor` audits broken links, orphaned entries, and repo issues
- **Conflict-safe** — refuses to overwrite unmanaged files without explicit confirmation
- **Lifecycle hooks** — run custom commands before/after syncing at repository or entry level with safe trust verification
- **Interactive 3-way merge** — cleanly merge changes between repo and local files with standard git-style conflict markers
- **Fingerprint-based status** — speed up sync checks using lightweight Blake2s-256 fingerprints
- **Shell completions** — tab-completion for bash, zsh, fish, elvish, and powershell

## How it works

dotling moves your config files into a central git repository and replaces them with symlinks (or copies). Each tracked file is recorded in a `dotling.toml` config at the repo root.

**Symlinks** (default): the deployed file points to the repo — edits are instantly reflected in your repo. `dotling sync` ensures the symlink is present and correct.

**Copies** (`--copy`): the deployed file is a standalone copy. Useful for apps that don't support symlinks. `dotling sync` compares content fingerprints and copies in whichever direction is newer.

Since dotling doesn't wrap git, you use native `git` commands to commit, push, and pull your dotfiles repo.

## Next steps

- [Getting Started](./getting-started.md) — install dotling and set up your first dotfiles repo
- [Configuration](./configuration.md) — understand the `dotling.toml` format
- [CLI Reference](./cli/README.md) — explore all available commands
