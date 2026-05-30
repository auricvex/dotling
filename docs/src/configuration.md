# Configuration

Tracked entries and settings are stored in `dotling.toml` at the repo root.

## Example

```toml
# dotling.toml — managed by dotling, safe to hand-edit

[settings]
method = "symlink" # Default deployment method: "symlink" or "copy"

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
permissions = "0600"

[[entries]]
source = "shell/bashrc"
target = "~/.bashrc"
os = "linux"
```

## Sections

### `[settings]`

Global settings that apply to all entries unless overridden per-entry.

| Field | Type | Default | Description |
|---|---|---|---|
| `method` | string | `"symlink"` | Default deployment method: `"symlink"` or `"copy"` |

### `[hooks]`

Global lifecycle hooks that run at the beginning and end of `dotling sync`.

| Field | Description |
|---|---|
| `init` | Command run during `dotling init` (also runs when adopting or cloning) |
| `before` | Command run before any entries are synced |
| `after` | Command run after all entries are successfully synced |

See [Lifecycle Hooks](./sync-details.md#lifecycle-hooks) for details on the hook trust system and environment variables.

### `[vars]`

Shared template variable defaults. These are committed to git and serve as documentation and fallbacks. Machine-specific overrides live in `~/.dotling/vars.toml` (never committed).

```toml
[vars]
hostname = "my-mac"       # placeholder — override locally
primary_user = "user"     # placeholder
```

See [Templates](./templates.md) for the full variable system.

### `[[entries]]`

Each tracked file or directory is an entry. The order in `dotling.toml` determines the sync order.

## Entry Fields

| Field | Type | Description |
|---|---|---|
| `source` | string | Repo-relative path (required) |
| `target` | string | Deploy target path with `~` support (required) |
| `method` | string | Override: `"symlink"` or `"copy"` |
| `encrypted` | boolean | `true`, `1`, or `yes` — marks entry as encrypted |
| `directory` | boolean | `true`, `1`, or `yes` — marks entry as a directory |
| `template` | boolean | `true`, `1`, or `yes` — marks entry as a template |
| `os` | string | `"all"` (default), `"linux"`, `"macos"`, or `"windows"` |
| `permissions` | string | Octal permissions applied on sync, e.g. `"0600"` |
| `before` | string | Entry-level hook run before sync |
| `after` | string | Entry-level hook run after sync |

> **Note:** Template entries are marked with `template: true` in `dotling.toml` (set automatically by `dotling add --template`).

## Path Mapping

Files are organized into categories automatically when added:

| Home path | Repo path |
|---|---|
| `~/.config/nvim/init.lua` | `config/nvim/init.lua` |
| `~/.zshrc` | `shell/zshrc` |
| `~/.bashrc` | `shell/bashrc` |
| `~/.gitconfig` | `git/gitconfig` |
| `~/.vimrc` | `vim/vimrc` |
| `~/.tmux.conf` | `tmux/tmux.conf` |
| `~/.ssh/config` | `ssh/config` |
| `~/.gnupg/gpg.conf` | `gnupg/gpg.conf` |
| `~/.somerc` | `home/somerc` |

The mapping rules are:

- `.config/*` files go to `config/`
- Shell files (`.zshrc`, `.bashrc`, `.bash_profile`, `.profile`, `.zprofile`, `.zshenv`, `.fishrc`) go to `shell/`
- Git files (`.gitconfig`, `.gitignore_global`) go to `git/`
- Vim files (`.vimrc`, `.gvimrc`) go to `vim/`
- Tmux files (`.tmux.conf`) go to `tmux/`
- SSH files (`.ssh/`) go to `ssh/`
- GnuPG files (`.gnupg/`) go to `gnupg/`
- Everything else goes to `home/`

## Multi-OS Support

Tag entries with `--os` to restrict them to a specific platform:

```sh
dotling add ~/.zshrc --os macos
dotling add ~/.bashrc --os linux
```

When deploying, dotling automatically skips entries that don't match the current OS. Entries without an `--os` flag (or tagged `all`) deploy everywhere.

Platform aliases: `darwin` = `macos`, `win` = `windows`.
