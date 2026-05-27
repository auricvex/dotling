<p align="center">
  <h1 align="center">dotling</h1>
  <p align="center">
    A zero-dependency dotfiles management CLI — track, link, and sync your config files across machines.
  </p>
</p>

<p align="center">
  <a href="https://crates.io/crates/dotling"><img alt="crates.io" src="https://img.shields.io/crates/v/dotling.svg?style=flat-square&logo=rust"></a>
  <a href="https://github.com/auricvex/dotling/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/auricvex/dotling/ci.yml?branch=main&style=flat-square&logo=github&label=CI"></a>
  <a href="https://github.com/auricvex/dotling/blob/main/LICENSE-MIT"><img alt="License" src="https://img.shields.io/crates/l/dotling?style=flat-square"></a>
  <a href="https://crates.io/crates/dotling"><img alt="Downloads" src="https://img.shields.io/crates/d/dotling?style=flat-square&color=blue"></a>
</p>

---

**dotling** v0.3.1 has been rebuilt from scratch. It moves your config files into a central git repository and replaces them with symlinks (or copies). It handles the tedious parts — path mapping, conflict detection, encryption, and multi-OS support — so you can set up a new machine in seconds.

## Features

- **Symlink & copy deployment** — choose per file, switch anytime
- **Automatic path mapping** — `~/.config/nvim` → `config/nvim`, `~/.zshrc` → `shell/zshrc`
- **Multi-OS support** — tag entries as `linux`, `macos`, or `windows`; skip irrelevant files automatically
- **Secure Password Vault** — encrypt sensitive files (API keys, .env) using an Argon2id + ChaCha20-Poly1305 Vault
- **Portable Secrets** — export your vault to easily unlock secrets on a new machine
- **Native Git integration** — dotling manages the symlinks, you manage the repo with native `git` commands
- **Health checks** — `dotling doctor` audits broken links, orphaned entries, and repo issues
- **Conflict-safe** — refuses to overwrite unmanaged files without explicit confirmation

## Installation

### From crates.io (recommended)

```sh
cargo install dotling
```

### Prebuilt binaries

Download a prebuilt binary from the [latest GitHub release](https://github.com/auricvex/dotling/releases/latest) for your platform:

| Platform | Binary |
|---|---|
| Linux (x86_64, glibc) | `dotling-x86_64-linux.tar.gz` |
| Linux (x86_64, musl) | `dotling-x86_64-linux-musl.tar.gz` |
| Linux (aarch64) | `dotling-aarch64-linux.tar.gz` |
| macOS (Intel) | `dotling-x86_64-macos.tar.gz` |
| macOS (Apple Silicon) | `dotling-aarch64-macos.tar.gz` |
| Windows (x86_64) | `dotling-x86_64-windows.zip` |
| Windows (ARM64) | `dotling-aarch64-windows.zip` |

```sh
# Example: Linux x86_64
curl -fsSL https://github.com/auricvex/dotling/releases/latest/download/dotling-x86_64-linux.tar.gz \
  | tar xz -C ~/.local/bin/
```

### Nix

```sh
nix run github:auricvex/dotling
# or add to your flake inputs
```

## Quick Start

```sh
# Initialize a new dotfiles repo
dotling init ~/dotfiles

# Or clone an existing one
dotling init git@github.com:you/dotfiles.git

# Track files
dotling add ~/.zshrc
dotling add ~/.config/nvim

# Deploy the symlinks
dotling deploy

# Since dotling doesn't wrap git, you can commit and push directly!
cd ~/dotfiles
git add .
git commit -m "initial setup"
git push
```

## Commands

| Command | Description |
|---|---|
| `dotling init <path\|url>` | Initialize a new repo or clone an existing one |
| `dotling add <paths>` | Move files into the repo and deploy a symlink back |
| `dotling remove <entries>` | Undeploy and remove from tracking |
| `dotling deploy` | Deploy all tracked entries (create symlinks or copies) |
| `dotling status` | Show deployment status of all tracked entries |
| `dotling encrypt <paths>` | Encrypt tracked entries using your Vault |
| `dotling decrypt <paths>` | Decrypt encrypted entries back to plaintext |
| `dotling vault <action>` | Manage your password-protected encryption Vault |
| `dotling doctor` | Audit repository health and report issues |

### Key Flags

| Command | Flag | Description |
|---|---|---|
| `all` | `-v, --verbose` | Show hints and additional details |
| `add` | `--copy` | Deploy as a copy instead of a symlink |
| `add` | `--encrypt` | Encrypt the file(s) using the vault password |
| `add` | `--os <platform>` | Target OS: `all`, `linux`, `macos`, `windows` |
| `remove` | `--purge` | Also delete the source files from the repo |
| `deploy` | `--force` | Overwrite conflicting files |
| `deploy` | `--dry-run` | Show what would change without modifying |
| `status` | `--diff` | Show inline diffs for modified copy entries |

## How It Works

dotling moves your config files into a central git repository and replaces them with symlinks (or copies). Each tracked file is recorded in a `dotling.toml` config at the repo root.

**Symlinks** (default): the deployed file points to the repo — edits are instantly reflected in your repo.

**Copies** (`--copy`): the deployed file is a standalone copy. Useful for apps that don't support symlinks.

### Path Mapping

Files are organized into categories automatically:

| Home path | Repo path |
|---|---|
| `~/.config/nvim/init.lua` | `config/nvim/init.lua` |
| `~/.zshrc` | `shell/zshrc` |
| `~/.gitconfig` | `git/gitconfig` |
| `~/.vimrc` | `vim/vimrc` |
| `~/.tmux.conf` | `tmux/tmux.conf` |
| `~/.somerc` | `home/somerc` |

### Configuration Format

Tracked entries are stored in `dotling.toml` at the repo root:

```toml
[[entries]]
source = "shell/zshrc"
target = "~/.zshrc"

[[entries]]
source = "config/nvim/init.lua"
target = "~/.config/nvim/init.lua"
method = "copy"

[[entries]]
source = "shell/bashrc"
target = "~/.bashrc"
os = "linux"
```

### Multi-OS Support

Tag entries with `--os` to restrict them to a specific platform:

```sh
dotling add ~/.zshrc --os macos
dotling add ~/.bashrc --os linux
```

When deploying, dotling automatically skips entries that don't match the current OS. Entries tagged `all` (the default) deploy everywhere.

### Encryption Vault

dotling v0.3.1 includes a built-in portable encryption Vault protected by Argon2id and ChaCha20-Poly1305. This lets you safely commit API keys, `.env` files, or ssh configs to your public dotfiles repo.

1. **Initialize your Vault:**
   ```sh
   dotling vault init
   ```
   You'll be prompted for a password. This creates a secure identity in `~/.dotling/vault/`.

2. **Add a file with encryption:**
   ```sh
   dotling add ~/.ssh/config --encrypt
   ```
   dotling will read your local file, encrypt it, store the ciphertext (`config.enc`) in your git repo, and deploy the decrypted file locally with secure permissions.

3. **Migrating to a new machine:**
   Export your vault bundle from your old machine:
   ```sh
   dotling vault export my-vault.bundle
   ```
   Then import it on the new machine:
   ```sh
   dotling vault import my-vault.bundle
   ```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Commit your changes (`git commit -m 'feat: add my feature'`)
4. Push to the branch (`git push origin feat/my-feature`)
5. Open a Pull Request

### Development

```sh
# Clone and build
git clone https://github.com/auricvex/dotling.git
cd dotling
nix develop --command cargo build

# Run tests
nix develop --command cargo test

# Lint
nix develop --command cargo clippy
nix develop --command cargo fmt --check
```

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE-2.0)

at your option.
