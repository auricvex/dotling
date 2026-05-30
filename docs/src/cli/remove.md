# dotling remove

Remove entries from tracking.

## Usage

```sh
dotling remove <ENTRIES>
```

## Arguments

| Argument | Description |
|---|---|
| `<ENTRIES>` | Source paths, target paths, or partial matches of entries to remove |

## Description

`dotling remove` undeploys tracked entries and restores files to their original locations. For each entry:

1. **Undeploy** — removes the symlink (or copy) from the target path
2. **Restore** — moves the file from the repo back to its original target location
3. **Decrypt** — if the entry was encrypted, decrypts the file before restoring
4. **Clean up** — removes empty parent directories in the repo
5. **Untrack** — removes the entry from `dotling.toml`

The restored file is placed at its original target path, so your config files remain intact after removing them from dotling.

## Examples

```sh
# Remove by source path
dotling remove shell/zshrc

# Remove by target path
dotling remove ~/.zshrc

# Remove multiple entries
dotling remove ~/.zshrc ~/.bashrc

# Partial match works too
dotling remove zshrc
```
