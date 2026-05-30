# Getting Started

## Installation

### From crates.io (recommended)

```sh
cargo install dotling
```

### Prebuilt binaries

Download a prebuilt binary from the [latest GitHub release](https://github.com/auricvex/dotling/releases/latest):

| Platform | Binary |
|---|---|
| Linux (x86_64, glibc) | `dotling-x86_64-linux.tar.gz` |
| Linux (x86_64, musl) | `dotling-x86_64-linux-musl.tar.gz` |
| Linux (aarch64) | `dotling-aarch64-linux.tar.gz` |
| macOS (Intel) | `dotling-x86_64-macos.tar.gz` |
| macOS (Apple Silicon) | `dotling-aarch64-macos.tar.gz` |

```sh
# Example: Linux x86_64
curl -fsSL https://github.com/auricvex/dotling/releases/latest/download/dotling-x86_64-linux.tar.gz \
  | tar xz -C ~/.local/bin/
```

### Homebrew (macOS & Linux)

```sh
brew tap auricvex/auricvex
brew install dotling
```

### Nix

```sh
nix run github:auricvex/dotling
```

## Quick Start

### 1. Initialize a dotfiles repo

```sh
# Create a new repo
dotling init ~/dotfiles

# Or clone an existing one
dotling init git@github.com:you/dotfiles.git
```

This creates the repo directory, initializes git, and writes a `dotling.toml` config file.

### 2. Track your config files

```sh
dotling add ~/.zshrc
dotling add ~/.config/nvim
```

dotling moves each file into the repo (organized by category) and deploys a symlink back to the original location. See [Path Mapping](./configuration.md#path-mapping) for how files are organized.

### 3. Sync everything

```sh
dotling sync
```

This pushes repo files to their actual locations (creating or fixing symlinks) and pulls any copy-mode entries that were modified locally.

### 4. Commit and push

Since dotling doesn't wrap git, use native commands:

```sh
cd ~/dotfiles
git add .
git commit -m "initial setup"
git push
```

### 5. Set up a new machine

```sh
# Clone your dotfiles repo
dotling init git@github.com:you/dotfiles.git

# Deploy everything
dotling sync

# If you have encrypted entries, import your vault first
dotling vault import my-vault.bundle
dotling sync
```

## Next steps

- [Configuration](./configuration.md) — learn about `dotling.toml` and entry options
- [Templates](./templates.md) — add machine-specific values to dotfiles
- [Encryption](./encryption.md) — encrypt sensitive files with the vault
- [CLI Reference](./cli/README.md) — explore all commands and flags
