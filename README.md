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

**dotling** is a zero-dependency dotfiles management CLI. It moves your config files into a central git repository and replaces them with symlinks (or copies). It handles the tedious parts — path mapping, conflict detection, encryption, templating, backups, hooks, and multi-OS support — so you can set up a new machine in seconds.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Commands](#commands)
- [How It Works](#how-it-works)
- [Configuration](#configuration)
- [Multi-OS Support](#multi-os-support)
- [Encryption Vault](#encryption-vault)
- [Dotfile Templating](#dotfile-templating)
- [Lifecycle Hooks](#lifecycle-hooks)
- [Backups & Conflict Resolution](#backups--conflict-resolution)
- [Sync Fingerprints](#sync-fingerprints)
- [Environment Variables](#environment-variables)
- [File Structure](#file-structure)
- [Contributing](#contributing)
- [License](#license)

## Features

- **Symlink & copy deployment** — choose per file, switch anytime
- **Bidirectional sync** — `dotling sync` pushes from repo → actual and pulls from actual → repo automatically
- **Automatic path mapping** — `~/.config/nvim` → `config/nvim`, `~/.zshrc` → `shell/zshrc`
- **Multi-OS support** — tag entries as `linux`, `macos`, or `windows`; skip irrelevant files automatically
- **Secure Password Vault** — encrypt sensitive files (API keys, .env) using an Argon2id + ChaCha20-Poly1305 Vault
- **Encrypted sync** — sync handles encrypted entries in both directions; modified plaintext is re-encrypted back into the repo automatically
- **Portable Secrets** — export your vault to easily unlock secrets on a new machine
- **Native Git integration** — dotling manages the symlinks, you manage the repo with native `git` commands
- **Dotfile Templating** — add machine-specific values (`hostname`, `username`, custom vars) to any dotfile using `{{ var.key }}` syntax; render on every sync via `~/.dotling/vars.toml`
- **Health checks** — `dotling doctor` audits broken links, orphaned entries, and repo issues
- **Conflict-safe** — refuses to overwrite unmanaged files without explicit confirmation
- **Automated Backups** — protects local files from accidental overwrites by saving them to chronological backup sessions
- **Lifecycle Hooks** — run custom commands before/after syncing at a repository or entry level with safe trust verification
- **Interactive 3-way Merge** — cleanly merge changes between repo and local files with standard git-style conflict markers
- **Fingerprint-based Status** — speed up encrypted and copy-mode sync checks using lightweight Blake2s-256 fingerprints, without prompting for passwords

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

# Or clone an existing one (clones to ~/dotfiles)
dotling init git@github.com:you/dotfiles.git

# Track files
dotling add ~/.zshrc
dotling add ~/.config/nvim

# Sync everything (repo → actual and actual → repo)
dotling sync

# Since dotling doesn't wrap git, you can commit and push directly!
cd ~/dotfiles
git add .
git commit -m "initial setup"
git push
```

## Commands

| Command | Description |
|---|---|
| `dotling init <path\|url>` | Initialize a new repo at the given path, or clone an existing repo from a URL (always clones to `~/dotfiles`) |
| `dotling add <paths>` | Move files into the repo and deploy a symlink (or copy) back |
| `dotling remove <entries>` | Undeploy, safely restore tracked files/folders recursively to their original target locations (decrypting if encrypted), and remove from tracking |
| `dotling sync` | Bidirectional sync — push repo → actual and pull actual → repo |
| `dotling status` | Show deployment status of all tracked entries |
| `dotling edit <entry>` | Edit any tracked entry in `$EDITOR` — decrypts/re-encrypts automatically for encrypted entries |
| `dotling encrypt <paths>` | Encrypt tracked entries using your Vault |
| `dotling decrypt <paths>` | Decrypt encrypted entries back to plaintext |
| `dotling vault <action>` | Manage your password-protected encryption Vault |
| `dotling doctor` | Audit repository health and report issues |
| `dotling vars <action>` | Manage machine-local template variables |
| `dotling backup <action>` | Manage local file backups created by dotling before overwriting |

### Key Flags

| Command | Flag | Description |
|---|---|---|
| `all` | `-v, --verbose` | Show hints and additional details |
| `add` | `--copy` | Deploy as a copy instead of a symlink |
| `add` | `--encrypt` | Encrypt the file(s) using the vault password |
| `add` | `--template` | Track as a template (`.dtmpl`): rendered on each sync with machine-local variables |
| `add` | `--os <platform>` | Target OS: `linux` (alias: `darwin`), `macos`, `windows` (alias: `win`). Omit for all platforms |
| `sync` | `--dry-run` | Preview changes without modifying anything |
| `sync` | `--force` | Overwrite conflicting files (repo wins; local backups created automatically) |
| `sync` | `--prefer-actual` | When both sides conflict, prefer the actual file (alias: `--prefer-local`) |
| `sync` | `--no-interactive` | Do not prompt for conflict resolution; skip conflicting entries and print a warning |
| `sync` | `--backup` | Always back up the local file before any push that would overwrite it |
| `sync` | `--allow-hooks` | Allow executing all hooks without prompting |
| `sync` | `--no-hooks` | Disable executing any hooks |
| `status` | `--diff` | Show inline diffs for modified copy entries |

## How It Works

dotling moves your config files into a central git repository and replaces them with symlinks (or copies). Each tracked file is recorded in a `dotling.toml` config at the repo root.

**Symlinks** (default): the deployed file points to the repo — edits are instantly reflected in your repo. `dotling sync` ensures the symlink is present and correct. Symlink entries are always repo-authoritative: if the symlink is wrong or missing, sync always pushes from repo to actual.

**Copies** (`--copy`): the deployed file is a standalone copy. Useful for apps that don't support symlinks. `dotling sync` compares content fingerprints first, then modification times, and copies in whichever direction is newer.

### Sync Direction

`dotling sync` decides the direction per entry:

| Entry type | Push (repo → actual) | Pull (actual → repo) |
|---|---|---|
| **Symlink** | Create/fix symlink | Never (symlink always reads repo) |
| **Copy** | Source newer or target missing | Target newer |
| **Encrypted** | `.enc` newer or target missing → decrypt | Target newer → re-encrypt into `.enc` |

When both sides differ and timestamps are equal, dotling defaults to **repo wins** (push). Pass `--prefer-actual` to flip this.

### Path Mapping

Files are organized into categories automatically:

| Home path | Repo path |
|---|---|
| `~/.config/nvim/init.lua` | `config/nvim/init.lua` |
| `~/.zshrc` | `shell/zshrc` |
| `~/.gitconfig` | `git/gitconfig` |
| `~/.vimrc` | `vim/vimrc` |
| `~/.tmux.conf` | `tmux/tmux.conf` |
| `~/.ssh/config` | `ssh/config` |
| `~/.gnupg/gpg.conf` | `gnupg/gpg.conf` |
| `~/.somerc` | `home/somerc` |

## Configuration

Tracked entries and settings are stored in `dotling.toml` at the repo root:

```toml
# dotling.toml — managed by dotling, safe to hand-edit

[settings]
method = "symlink" # Default sync method: "symlink" or "copy"

[hooks]
init = "echo 'Initializing repo...'"
before = "echo 'Starting global before-sync hook...'"
after = "echo 'Global after-sync hook completed.'"

[vars]
hostname = "my-mac"       # shared default — override in ~/.dotling/vars.toml
primary_user = "user"     # placeholder

[[entries]]
source = "shell/zshrc"
target = "~/.zshrc"
before = "echo 'Updating zshrc...'"
after = "echo 'zshrc updated!'"

[[entries]]
source = "config/nvim/init.lua"
target = "~/.config/nvim/init.lua"
method = "copy"
permissions = "0600" # Apply octal permissions on sync

[[entries]]
source = "shell/bashrc"
target = "~/.bashrc"
os = "linux"
```

### Entry Fields

| Field | Type | Description |
|---|---|---|
| `source` | string | Repo-relative path (required) |
| `target` | string | Deploy target path with `~` support (required) |
| `method` | string | Override: `"symlink"` or `"copy"` |
| `encrypted` | boolean | `true`, `1`, or `yes` — marks entry as encrypted |
| `os` | string | `"all"` (default), `"linux"`, `"macos"`, or `"windows"` |
| `permissions` | string | Octal permissions applied on sync, e.g. `"0600"` |
| `before` | string | Entry-level hook run before sync |
| `after` | string | Entry-level hook run after sync |

> **Note:** Template entries don't have a `template` field. A file is treated as a template when its `source` path ends with `.dtmpl` (e.g. `shell/zshrc.dtmpl`). This suffix is added automatically by `dotling add --template`.

### Multi-OS Support

Tag entries with `--os` to restrict them to a specific platform:

```sh
dotling add ~/.zshrc --os macos
dotling add ~/.bashrc --os linux
```

When deploying, dotling automatically skips entries that don't match the current OS. Entries without an `--os` flag (or tagged `all`) deploy everywhere.

Platform aliases: `darwin` = `macos`, `win` = `windows`.

## Encryption Vault

dotling includes a built-in portable encryption Vault protected by Argon2id and ChaCha20-Poly1305. This lets you safely commit API keys, `.env` files, or ssh configs to your public dotfiles repo.

### 1. Initialize your Vault

```sh
dotling vault init
```

You'll be prompted for a password (entered twice for confirmation). This creates a secure identity in `~/.dotling/vault/`.

### 2. Add a file with encryption

```sh
dotling add ~/.ssh/config --encrypt
```

dotling will read your local file, encrypt it, store the ciphertext (`config.enc`) in your git repo, and deploy the decrypted file locally with secure permissions.

### 3. Sync encrypted entries

`dotling sync` handles encrypted entries in both directions. If you edit the deployed plaintext file, running `sync` will re-encrypt it back into the repo:

```sh
# Edit your deployed file, then sync it back
vim ~/.ssh/config
dotling sync   # detects the file is newer → re-encrypts into ssh/config.enc
```

### 4. Migrating to a new machine

Export your vault bundle from your old machine:

```sh
dotling vault export my-vault.bundle
```

Then import it on the new machine and sync:

```sh
dotling vault import my-vault.bundle
dotling sync
```

### 5. Editing encrypted files

Use `dotling edit` to open an encrypted file in your `$EDITOR` without a manual decrypt/encrypt cycle:

```sh
# Opens the decrypted content in $EDITOR, re-encrypts on save
dotling edit ~/.ssh/config

# Works with any tracked path: source path, target path, or partial name
dotling edit ssh/config
```

After editing, run `dotling sync` to push the re-encrypted changes to your deployed file.

**Editor resolution:** `$DOTLING_EDITOR` → `$VISUAL` → `$EDITOR` → `vim` → `nano` → `vi`. GUI editors (`code`, `subl`, `zed`, `pulsar`, `atom`) automatically get `--wait` appended.

**Secure temp files:** Decrypted content is written to `~/.dotling/tmp/` with restricted permissions (mode `0600`). Temp files are securely wiped (overwritten with zeros) before deletion.

### 6. Other vault commands

```sh
dotling vault show             # Display vault status and location
dotling vault change-password  # Change the vault password
```

## Dotfile Templating

Some dotfiles contain machine-specific values — a hostname in a Nix flake, a username in a config, a path that differs per machine. dotling supports opt-in templating to handle this cleanly.

### How it works

Any file tracked with `--template` is stored in the repo as `<name>.dtmpl`. On every `sync`, dotling renders the template and writes the output to the deploy target — the repo source is never deployed directly.

```sh
# 1. Set your machine-local variables (saved to ~/.dotling/vars.toml, never committed)
dotling vars set hostname "Macbook-Air-Ade"
dotling vars set primary_user "ade"

# 2. Add a file as a template
dotling add ~/.config/nix-darwin/flake.nix --template

# 3. On another machine, sync will detect missing vars and prompt for them
dotling sync
```

### Template syntax

```nix
# ~/.config/nix-darwin/flake.nix.dtmpl
darwinConfigurations = {
  {{ var.hostname }} = darwin.lib.darwinSystem { ... };
};
```

```toml
# ~/.config/nix-darwin/configuration.nix.dtmpl
system.primaryUser = "{{ var.primary_user }}";
```

| Expression | Description |
|---|---|
| `{{ var.key }}` | User-defined variable (local or config default) |
| `{{ dotling.hostname }}` | Current machine hostname |
| `{{ dotling.username }}` | Current OS username |
| `{{ dotling.os }}` | `macos`, `linux`, or `windows` |
| `{{ dotling.arch }}` | `x86_64`, `aarch64`, or `arm` |
| `{{ dotling.home }}` | Home directory path |
| `{{ dotling.repo }}` | Dotfiles repo root path |
| `{{ env.VAR }}` | Environment variable |
| `{{ var.key \| upper }}` | Apply a filter (`upper`, `lower`, `trim`, `quote`, `squote`) |
| `{{ var.key \| default "fallback" }}` | Use a fallback if the variable is not set |
| `{{- expr -}}` | Strip surrounding whitespace (also `{{- expr }}` and `{{ expr -}}` for one-sided trimming) |

### Variable sources

Variables are resolved in priority order:

1. **Local store** — `~/.dotling/vars.toml` (machine-specific, never committed)
2. **Config defaults** — `[vars]` in `dotling.toml` (shared, committed)

Built-in variables (`dotling.*`) and environment variables (`env.*`) are separate namespaces resolved directly.

Shared defaults in `dotling.toml` act as documentation and fallbacks — use placeholders, not real values:

```toml
# dotling.toml
[vars]
hostname = "my-mac"       # placeholder — override in ~/.dotling/vars.toml
primary_user = "user"     # placeholder
```

### Encrypted templates

Sensitive templates (e.g. a config containing tokens) can be both templated and encrypted:

```sh
dotling add ~/.config/secret.conf --template --encrypt
```

The pipeline on sync is: `vault decrypt → render with vars → deploy`.

### `dotling vars` reference

```sh
dotling vars list                    # show all resolved variables (tagged [auto], [local], or [default])
dotling vars set hostname "my-mac"   # set a machine-local variable
dotling vars get hostname            # print the resolved value (checks builtins, then var namespace)
dotling vars unset hostname          # remove from local store
dotling vars check                   # validate all templates for unresolved variables
dotling vars import ~/.env           # bulk-import from .env or TOML file
dotling vars export                  # print local variables as TOML (for new machines)
```

## Lifecycle Hooks

dotling supports executing hooks (shell commands) globally or per-entry during the sync process. Hooks can be used for actions like reloading your shell, compiling configurations, or running custom setup scripts.

### Global Hooks

Global hooks run at the very beginning and very end of the `dotling sync` session:

- `init`: Command run during repository initialization (also runs when adopting or cloning an existing repo).
- `before`: Command run before any entries are synced.
- `after`: Command run after all entries are successfully synced.

### Entry-level Hooks

You can define hooks specific to individual tracked entries:

- `before`: Command run before this entry is pushed or pulled.
- `after`: Command run after this entry is successfully pushed or pulled.

### Execution Context

Hooks are executed in the repository root directory. The following environment variables are populated to provide rich runtime context:

| Variable | Description |
|---|---|
| `DOTLING_HOOK_TYPE` | Type of hook: `global_init`, `global_before`, `global_after`, `entry_before`, `entry_after` |
| `DOTLING_REPO_ROOT` | Absolute path to the dotfiles repository |
| `DOTLING_DRY_RUN` | `"true"` if running with `--dry-run`, otherwise `"false"` |
| `DOTLING_ENTRY_SOURCE` | (Entry hooks only) Repo-relative path of the entry's source file/folder |
| `DOTLING_ENTRY_TARGET` | (Entry hooks only) Target path of the entry's deployed file/folder |
| `DOTLING_ENTRY_ACTION` | (Entry hooks only) Current action being performed (`"push"` or `"pull"`) |

### Hook Trust System

To protect against malicious code in imported dotfile repositories, dotling prompts for user verification before running a hook for the first time:

```text
  ⚡ Untrusted hook detected (type: entry_before):
    echo "updating shell configuration"
    ? Do you want to run this hook? [y]es (once) / [n]o (skip) / [a]lways (trust) / [s]kip all >
```

Selecting `always` stores the Blake2s-256 hash of the command string in `~/.dotling/state/trusted_hooks` so it runs seamlessly on subsequent syncs.

- Pass `--allow-hooks` (or set `DOTLING_ALLOW_HOOKS=1`) to automatically execute all hooks without prompting.
- Pass `--no-hooks` (or set `DOTLING_NO_HOOKS=1`) to completely disable hook execution.

### Hook Retry

If a hook command exits with a non-zero status, dotling automatically retries it up to **3 times** (1 initial attempt + 2 retries) before aborting the sync. A warning is printed after each failed attempt so the failure is always visible.

## Backups & Conflict Resolution

### Backups

To protect your local environment from accidental data loss, dotling automatically backs up files before they are overwritten:

- Backups are stored in `~/.dotling/backups/<unix-seconds>/<repo-relative-source-path>`.
- Pass `--backup` to the sync command to always force a local backup before any push that would overwrite a file, even when there is no conflict.
- `--force` also creates backups automatically before overwriting.
- List backup sessions using `dotling backup list`.
- Prune old backups using `dotling backup clean [--keep-last N] [--older-than DAYS]`. By default, clean keeps the 10 most recent sessions.

### Conflict Resolution & Three-way Merge

When sync detects a conflict between the repository and your local target, you can choose from the following interactive options:

- `[s]` Show diff: Compare inline changes.
- `[k]` Keep Local: Overwrite the repository with your local file (pulls to repo).
- `[r]` Use Repo: Overwrite the local file with the repository version (pushes to local, backs up the local file first).
- `[m]` Merge: Performs a standard line-level **three-way merge** using the last-in-sync snapshot as the base, combining modifications from both the repo (ours) and local target (theirs). Non-overlapping changes are cleanly auto-merged, while overlapping conflicts are highlighted with standard git conflict markers:
  ```text
  <<<<<<< repo
  repo version content
  =======
  actual local content
  >>>>>>> actual
  ```
  The merge outcome is written back to both the local disk and the repository, resolving the conflict.
- `[x]` Skip: Leave this entry unresolved and continue.

Conflict types dotling detects:

- **First seen** — a file exists at the target path but was never tracked by dotling (pre-existing file).
- **Both modified** — both the repo and local file changed since the last sync.
- **Timestamp tie** — files differ but modification times are identical.

Snapshots used as the merge base are stored in `~/.dotling/snapshots/<source>` and are updated after each successful sync of a copy-mode plain file.

## Sync Fingerprints

Previously, encrypted entries had to be decrypted to verify their sync state. dotling v0.5.0 introduces lightweight Blake2s-256 sync fingerprints stored in `~/.dotling/fingerprints.toml`.

- After each successful sync, dotling records the content hashes of the `.enc` ciphertext and the local plaintext target.
- On subsequent `status` or `sync` checks, dotling compares current file hashes against the stored fingerprint.
- **Benefits:** You can run `dotling status` or `dotling sync --dry-run` to audit your system instantly, without entering your vault password. A password is only requested when actual file modifications need to be decrypted or re-encrypted!
- For copy-mode plain files, fingerprints track both repo source and target file hashes, enabling deterministic detection of which side has changed.

## Environment Variables

dotling reads and respects the following environment variables:

| Variable | Description |
|---|---|
| `NO_COLOR` | Disables ANSI color output when set (follows the [no-color.org](https://no-color.org) standard) |
| `DOTLING_EDITOR` | Highest-priority editor override for `dotling edit` |
| `VISUAL` | Editor override (standard Unix convention) |
| `EDITOR` | Editor fallback |
| `DOTLING_ALLOW_HOOKS` | Set to `1` or `true` to auto-trust and execute all hooks without prompting |
| `DOTLING_NO_HOOKS` | Set to `1` or `true` to completely disable hook execution |
| `HOSTNAME` | Fallback hostname for `dotling.hostname` built-in if the syscall fails |
| `USER` / `USERNAME` | Used for `dotling.username` built-in (`USERNAME` is the Windows fallback) |

## File Structure

dotling stores all state under `~/.dotling/`:

| Path | Purpose |
|---|---|
| `state.toml` | Registered repo root path |
| `vars.toml` | Machine-local template variables (never committed) |
| `fingerprints.toml` | Sync fingerprint store for change detection |
| `vault/` | Encryption vault (`identity.enc`, `config.toml`) |
| `backups/` | Chronological backup sessions |
| `snapshots/` | Plaintext snapshots used as merge base for three-way merge |
| `state/trusted_hooks` | Blake2s-256 hashes of trusted hook commands |
| `tmp/` | Secure temp files for encrypted file editing (auto-cleaned) |

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines, development setup, and PR workflow.

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE-2.0)

at your option.
