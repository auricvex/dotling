# dotling vars

Manage machine-local template variables.

## Usage

```sh
dotling vars <ACTION>
```

## Actions

### `dotling vars list`

Show all resolved variables (built-in, config defaults, and local).

```sh
dotling vars list
```

Variables are tagged with their source: `[auto]` for built-ins, `[local]` for machine-local, `[default]` for config defaults.

### `dotling vars set`

Set a machine-local variable in `~/.dotling/vars.toml`.

```sh
dotling vars set <KEY> <VALUE>
```

### `dotling vars get`

Print the resolved value of a single variable.

```sh
dotling vars get <KEY>
```

Checks built-ins first, then local store, then config defaults.

### `dotling vars unset`

Remove a variable from the local store.

```sh
dotling vars unset <KEY>
```

### `dotling vars check`

Validate all template entries for unresolved variables.

```sh
dotling vars check
```

Reports any template variables that don't have a value in either the local store or config defaults.

### `dotling vars import`

Bulk-import variables from a TOML or `.env` file.

```sh
dotling vars import <PATH>
```

Accepts either a TOML file with a `[vars]` section or a `.env` file with `KEY=VALUE` pairs.

### `dotling vars export`

Print local variables as TOML (useful for migrating to a new machine).

```sh
dotling vars export
```

Output can be redirected to a file and imported on another machine with `dotling vars import`.

## Variable resolution priority

1. **Local store** — `~/.dotling/vars.toml` (machine-specific, never committed)
2. **Config defaults** — `[vars]` in `dotling.toml` (shared, committed)

Built-in variables (`dotling.*`) and environment variables (`env.*`) are separate namespaces.

## Examples

```sh
# Set machine-specific values
dotling vars set hostname "work-laptop"
dotling vars set primary_user "alice"

# Check what's configured
dotling vars list
dotling vars get hostname

# Validate templates
dotling vars check

# Migrate to new machine
dotling vars export > vars-backup.toml
# ... on new machine ...
dotling vars import vars-backup.toml

# Import from .env file
dotling vars import ~/.env
```

## See also

- [Templates](../templates.md) — full template syntax and variable system
