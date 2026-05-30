# dotling status

Show deployment status of all tracked entries.

## Usage

```sh
dotling status [OPTIONS]
```

## Options

| Flag | Description |
|---|---|
| `--diff` | Show inline diffs for modified copy entries |

## Description

`dotling status` displays the current state of each tracked entry, grouped by category. It checks whether each entry is properly deployed, up to date, or has issues.

### Status indicators

| Status | Meaning |
|---|---|
| `Deployed` | Entry is correctly deployed and in sync |
| `Modified` | Entry has local changes that differ from the repo |
| `Missing` | Target file is missing |
| `Broken` | Symlink points to a non-existent target |
| `Conflict` | Both repo and local file have changed |

### Sync badges

| Badge | Meaning |
|---|---|
| `[in sync]` | Repository and target match |
| `[needs sync]` | One side has changed, run `dotling sync` |
| `[diff]` | Content differs (shown with `--diff`) |

### Fingerprints

For copy-mode and encrypted entries, status checks use Blake2s-256 fingerprints stored in `~/.dotling/fingerprints.toml`. This means you can check status without entering your vault password — it's only needed when actual file modifications are required.

## Examples

```sh
# Show status of all entries
dotling status

# Show inline diffs for modified entries
dotling status --diff
```
