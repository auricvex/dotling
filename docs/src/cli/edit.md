# dotling edit

Edit a tracked entry in your `$EDITOR`.

## Usage

```sh
dotling edit <ENTRY>
```

## Arguments

| Argument | Description |
|---|---|
| `<ENTRY>` | Source path, target path, or partial match of the entry to edit |

## Description

`dotling edit` opens a tracked entry in your preferred text editor. It accepts any identifier that uniquely matches an entry: the repo source path, the target path, or a partial name match.

### Plain and template entries

For plain and template entries, the repo source file is opened directly. Changes are saved to the repo — run `dotling sync` to deploy them.

### Encrypted entries

For encrypted entries, dotling:

1. Decrypts the file to a secure temp file in `~/.dotling/tmp/` (mode `0600`)
2. Opens the temp file in your editor
3. Re-encrypts the content and writes it back to the repo
4. Securely wipes (overwrites with zeros) and deletes the temp file

### Editor resolution

The editor is resolved in this order:

1. `$DOTLING_EDITOR` — highest priority
2. `$VISUAL` — standard Unix convention
3. `$EDITOR` — standard fallback
4. `vim` -> `nano` -> `vi` — hardcoded fallbacks

GUI editors (`code`, `subl`, `zed`, `pulsar`, `atom`) automatically get `--wait` appended so dotling waits for the editor to close before re-encrypting.

## Examples

```sh
# Edit by target path
dotling edit ~/.ssh/config

# Edit by source path
dotling edit ssh/config

# Partial match
dotling edit zshrc

# Use a specific editor
DOTLING_EDITOR=nvim dotling edit ~/.gitconfig
```
