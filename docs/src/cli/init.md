# dotling init

Initialize a new dotfiles repo or adopt an existing one.

## Usage

```sh
dotling init [PATH|URL]
```

**Default:** `~/dotfiles`

## Description

`dotling init` sets up a dotfiles repository. Depending on the argument:

- **A local path** — creates the directory, writes a default `dotling.toml`, runs `git init`, and registers the repo root
- **A git URL** — clones the repo to `~/dotfiles`, registers the repo root, and suggests running `dotling sync` to deploy entries

After initialization, the repo root is stored in `~/.dotling/state.toml` so dotling knows where to find it.

## What happens

1. Creates the repo directory (or clones from URL)
2. Writes a default `dotling.toml` with empty sections
3. Runs `git init` (for new repos)
4. Registers the repo root in `~/.dotling/state.toml`
5. Runs the `[hooks] init` command if defined
6. For cloned repos: suggests running `dotling sync` to deploy entries

## Examples

```sh
# Create a new dotfiles repo
dotling init ~/dotfiles

# Create at the default location
dotling init

# Clone an existing repo
dotling init git@github.com:you/dotfiles.git
dotling init https://github.com/you/dotfiles.git
```
