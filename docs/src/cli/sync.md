# dotling sync

Synchronise tracked entries between the repo and the actual filesystem.

## Usage

```sh
dotling sync [OPTIONS]
```

## Options

| Flag | Description |
|---|---|
| `--dry-run` | Show what would change without modifying anything |
| `--force` | Overwrite conflicting files without prompting (repo wins) |
| `--prefer-actual` | When both sides differ, prefer the local file (alias: `--prefer-local`) |
| `--no-interactive` | Skip conflicting entries and print a warning |
| `--allow-hooks` | Execute all hooks without prompting |
| `--no-hooks` | Disable all hook execution |

## Description

`dotling sync` is the core command that keeps your repo and actual filesystem in sync. It processes all entries in `dotling.toml`:

- **Symlink entries** — ensures the symlink exists and points to the correct repo file
- **Copy entries** — compares content fingerprints and copies in the newer direction
- **Encrypted entries** — decrypts or re-encrypts based on which side is newer
- **Template entries** — renders the template and deploys the output

See [Sync](../sync-details.md) for the full sync process, conflict resolution, and hook system.

## Examples

```sh
# Sync everything
dotling sync

# Preview changes
dotling sync --dry-run

# Force repo version on all conflicts
dotling sync --force

# Prefer local files on conflicts
dotling sync --prefer-actual

# Non-interactive (for scripts/CI)
dotling sync --no-interactive

# Trust all hooks
dotling sync --allow-hooks

# Skip all hooks
dotling sync --no-hooks
```
