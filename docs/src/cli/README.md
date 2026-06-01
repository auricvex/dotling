# CLI Reference

dotling is organized as a set of subcommands. Each command has its own page with detailed usage, flags, and examples.

## Commands

| Command | Description |
|---|---|
| [dotling init](./init.md) | Initialize a new repo or clone an existing one |
| [dotling add](./add.md) | Move files into the repo and deploy symlinks/copies |
| [dotling remove](./remove.md) | Untrack entries and restore files to original locations |
| [dotling sync](./sync.md) | Bidirectional sync between repo and actual filesystem |
| [dotling status](./status.md) | Show deployment status of all tracked entries |
| [dotling edit](./edit.md) | Edit a tracked entry in `$EDITOR` |
| [dotling encrypt](./encrypt.md) | Encrypt tracked entries |
| [dotling decrypt](./decrypt.md) | Decrypt encrypted entries |
| [dotling vault](./vault.md) | Manage the encryption vault |
| [dotling doctor](./doctor.md) | Audit repository health |
| [dotling vars](./vars.md) | Manage machine-local template variables |
| [dotling completions](./completions.md) | Generate shell completion scripts |

## Global flags

| Flag | Description |
|---|---|
| `-v, --verbose` | Show hints and additional details |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Key command flags

| Command | Flag | Description |
|---|---|---|
| `add` | `--copy` | Deploy as a copy instead of a symlink |
| `add` | `--encrypt` | Encrypt the file(s) using the vault password |
| `add` | `--template` | Track as a template: rendered on each sync |
| `add` | `--os <platform>` | Target OS: `linux`, `macos`, `windows` |
| `sync` | `--dry-run` | Preview changes without modifying anything |
| `sync` | `--force` | Overwrite conflicting files (repo wins) |
| `sync` | `--prefer-actual` | Prefer the local file on conflict |
| `sync` | `--no-interactive` | Skip conflicts, print warnings |
| `sync` | `--allow-hooks` | Execute all hooks without prompting |
| `sync` | `--no-hooks` | Disable all hook execution |
| `status` | `--diff` | Show inline diffs for modified copy entries |
