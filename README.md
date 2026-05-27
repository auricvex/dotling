<p align="center">
  <h1 align="center">dotling</h1>
  <p align="center">
    A dotfiles management CLI — track, link, and sync your config files across machines.
  </p>
</p>

<p align="center">
  <a href="https://crates.io/crates/dotling"><img alt="crates.io" src="https://img.shields.io/crates/v/dotling.svg?style=flat-square&logo=rust"></a>
  <a href="https://github.com/auricvex/dotling/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/auricvex/dotling/ci.yml?branch=main&style=flat-square&logo=github&label=CI"></a>
  <a href="https://github.com/auricvex/dotling/blob/main/LICENSE-MIT"><img alt="License" src="https://img.shields.io/crates/l/dotling?style=flat-square"></a>
  <a href="https://crates.io/crates/dotling"><img alt="Downloads" src="https://img.shields.io/crates/d/dotling?style=flat-square&color=blue"></a>
</p>

---

**dotling** moves your config files into a central git repository and replaces them with symlinks (or copies). It handles the tedious parts — path mapping, conflict detection, multi-OS support — so you can set up a new machine in seconds.

## Features

- **Symlink & copy deployment** — choose per file, switch anytime
- **Automatic path mapping** — `~/.config/nvim` → `config/nvim`, `~/.zshrc` → `shell/zshrc`
- **Multi-OS support** — tag entries as `linux`, `macos`, or `windows`; skip irrelevant files automatically
- **Secure Encryption** — natively encrypt sensitive files (API keys, .env) using `age`
- **Git-integrated** — init, commit, push, pull, and sync in one workflow
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
dotling link ~/.zshrc
dotling link ~/.config/nvim

# Push to remote
dotling push "initial setup"

# On another machine, clone and deploy
dotling init git@github.com:you/dotfiles.git
```

## Commands

| Command | Description |
|---|---|
| `dotling init <path\|url>` | Initialize a new repo or clone an existing one |
| `dotling link <path>` | Move a file into the repo and deploy a symlink back |
| `dotling unlink <path>` | Undeploy and remove from tracking |
| `dotling keygen` | Generate a new age encryption keypair |
| `dotling sync` | Pull changes from remote and re-deploy entries |
| `dotling push [message]` | Stage, commit, and push all changes |
| `dotling status` | Show deployment status of all tracked entries |
| `dotling diff [file]` | Show diff between repo source and deployed file |
| `dotling apply` | Re-deploy missing or broken entries |
| `dotling pull-back <file>` | Copy a deployed file back into the repo |
| `dotling list` | List all tracked entries grouped by category |
| `dotling doctor` | Audit repository health and report issues |

### Flags

| Flag | Commands | Description |
|---|---|---|
| `-v, --verbose` | all | Show hints and additional details |
| `--as-dir` | link | Treat directory as a single symlink unit |
| `--copy` | link | Deploy as a copy instead of a symlink |
| `--encrypt` | link | Deploy as an age-encrypted file |
| `--no-commit` | link | Skip automatic git commit |
| `--os <platform>` | link | Target OS: `all`, `linux`, `macos`, `windows` |
| `--save` | keygen | Save generated secret key to config directory |
| `--purge` | unlink | Also delete the source file from the repo |
| `--push` | sync | Push local changes before pulling |
| `--force` | sync | Overwrite modified copies during re-apply |
| `--dry-run` | sync, apply | Show what would change without modifying |

## How It Works

dotling moves your config files into a central git repository and replaces them with symlinks (or copies). Each tracked file is recorded in a `.dotling.toml` config at the repo root.

**Symlinks** (default): the deployed file points to the repo — edits are instantly reflected.

**Copies** (`--copy`): the deployed file is a standalone copy — use `dotling pull-back` to sync changes back.

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

Tracked entries are stored in `.dotling.toml` at the repo root:

```toml
[[links]]
src = "shell/zshrc"
dest = "~/.zshrc"

[[links]]
src = "config/nvim/init.lua"
dest = "~/.config/nvim/init.lua"
method = "copy"

[[links]]
src = "shell/bashrc"
dest = "~/.bashrc"
os = "linux"
```

### Multi-OS Support

Tag entries with `--os` to restrict them to a specific platform:

```sh
dotling link ~/.zshrc --os macos
dotling link ~/.bashrc --os linux
```

When deploying, dotling automatically skips entries that don't match the current OS. Entries tagged `all` (the default) deploy everywhere.

### Encryption

dotling includes built-in, secure encryption using [age](https://github.com/FiloSottile/age). This lets you safely commit API keys, `.env` files, or ssh configs to your public dotfiles repo.

1. **Generate a keypair:**
   ```sh
   dotling keygen --save
   ```
   This generates a new x25519 keypair, saving your secret key in `~/.dotling/identities/default.txt`, and printing your public key.

2. **Add the public key to `.dotling.toml`:**
   ```toml
   [encryption]
   recipients = [
       "age1yur9...",
   ]
   ```

3. **Link a file with encryption:**
   ```sh
   dotling link ~/.ssh/config --encrypt
   ```
   dotling will read your local file, encrypt it with your configured public key, store the ciphertext in your git repo, and deploy the decrypted file locally.

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
cargo build

# Run tests
cargo test

# Lint
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE-2.0)

at your option.
