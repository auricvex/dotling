# Changelog

All notable changes to [Dotling](https://github.com/auricvex/dotling) are documented here.

Each release follows [Keep a Changelog](https://keepachangelog.com/) conventions: **Added**, **Changed**, **Fixed**, **Removed**.

---

## v0.7.0

### Added

- **Shell completions** (`dotling completions <SHELL>`)
  - Generate tab-completion scripts for **bash**, **zsh**, **fish**, **elvish**, and **powershell**.
  - Completions are generated at runtime from the CLI definition using `clap_complete`, so they stay in sync as commands and flags evolve.
  - Output is written to stdout — redirect to your shell's completion directory to activate.
  - Quick install: `just install-completions` auto-detects your shell and writes to the conventional location.

---

## v0.6.2

### Changed

- **Module restructuring** — Reorganized the codebase from a flat module layout into a layered architecture with three top-level domain modules:
  - `core/` — foundational utilities: `error`, `fs`, `path`, `platform`, `store`
  - `config/` — configuration and templating: `config`, `template`, `vars`
  - `sync/` — sync engine: `backup`, `deploy`, `fingerprint`, `hooks`, `merge`
- Added `lib.rs` with public re-exports so downstream paths (`dotling::error`, `dotling::ui`, etc.) resolve correctly.
- `main.rs` now imports via the `dotling::` crate path instead of bare module names.

### Fixed

- Minor formatting cleanup in the sync command hook error message.

---

## v0.6.1

### Added

- **`dotling edit <entry>`** — Open any tracked file in your editor without manually decrypting and re-encrypting.
  - Encrypted entries are decrypted to a secure temp file, opened in your editor, then re-encrypted on save.
  - Plain, copy, and template entries open the repo source directly.
  - Lookup by source path, target path, or partial match.
  - Editor priority: `$DOTLING_EDITOR` → `$VISUAL` → `$EDITOR` → `vim` → `nano`.
  - Run `dotling sync` afterward to deploy your changes.

### Fixed

- **Hook retry logic** — Hooks that fail are now retried up to 3 times before the sync is aborted. A warning is printed after each failed attempt so you always know what happened.

---

## v0.6.0

### Added

- **Dotfile templating** (`dotling add --template`, `dotling vars`)
  - Render machine-specific values into your dotfiles automatically. Source files use `.dtmpl` suffix and are rendered on every `sync`.
  - **Template syntax** — `{{ var.key }}` for custom variables, `{{ dotling.hostname }}` / `username` / `os` / `arch` / `home` / `repo` for built-ins, `{{ env.VAR }}` for environment variables.
  - **Pipe filters** — `upper`, `lower`, `trim`, `quote`, `squote`, `default:fallback` (e.g. `{{ var.name | upper }}`).
  - **Whitespace control** — `{{- expr -}}` strips surrounding whitespace.
  - **Hard errors on missing variables** — `sync` and `add` abort with a helpful message and hint to run `dotling vars set`.
  - **Variable storage** — Per-machine values in `~/.dotling/vars.toml` (never committed). Shared defaults in `[vars]` in `dotling.toml` (committed). Local values win.
  - **`dotling add <path> --template`** — Validates syntax, copies as `.dtmpl`, renders immediately, and deploys. Combine with `--encrypt` to store encrypted.
  - **`dotling vars`** subcommand with seven actions:
    - `list` — all resolved variables with their source.
    - `set <key> <value>` — save a machine-local variable.
    - `get <key>` — print a single variable's value and source.
    - `unset <key>` — remove a local variable.
    - `check` — validate templates and report missing variables.
    - `import <path>` — bulk-import from `.toml` or `.env`.
    - `export` — print local variables as TOML for migrating to a new machine.
  - **Bootstrap prompt** — First `sync` on a new machine detects missing variables and asks for them interactively.
  - **Fingerprint-aware sync** — Templates skip push/pull conflict logic; output is only rewritten when the rendered content actually changes.
  - **`dotling status`** — Template entries show a `📄` badge and fingerprint-based drift detection.
  - **`dotling doctor`** — Reports unresolved variables per template entry with fix hints.
  - **Security** — Per-machine secrets stay in `~/.dotling/vars.toml` (never committed). Encrypted templates use the pipeline: decrypt → render → deploy.

---

## v0.5.0

### Added

- **Backups** (`dotling backup`)
  - Automatic backups before overwriting local files. Stored under `~/.dotling/backups/<timestamp>/<path>`.
  - `dotling backup list` — view all backup sessions.
  - `dotling backup clean [--keep-last N] [--older-than DAYS]` — prune old backups (keeps 10 most recent by default).
  - `--backup` flag on `sync` to force a backup before any overwrite.

- **Lifecycle hooks**
  - Run custom commands before/after sync, either globally or per-entry.
  - Global hooks: `[hooks]` block with `init`, `before`, `after`.
  - Per-entry hooks: `before` and `after` properties on each entry.
  - Untrusted hooks are verified interactively — choose to run once, skip, skip all, or always trust (stored as a Blake2s hash).
  - Rich context available to hooks: `DOTLING_HOOK_TYPE`, `DOTLING_REPO_ROOT`, `DOTLING_DRY_RUN`, `DOTLING_ENTRY_SOURCE`, `DOTLING_ENTRY_TARGET`, `DOTLING_ENTRY_ACTION`.
  - CLI flags: `--allow-hooks` / `--no-hooks`. Env vars: `DOTLING_ALLOW_HOOKS=1` / `DOTLING_NO_HOOKS=1`.

- **Line-level three-way merge**
  - Interactive `[m]erge` option for copy-mode files during conflict resolution.
  - Uses three-way merge against stored baselines, with git-style conflict markers for overlapping changes.

- **Sync fingerprints**
  - `~/.dotling/fingerprints.toml` tracks content hashes for encrypted and copy-mode entries.
  - Enables quick status checks without needing the vault password.
  - Detects where changes happened: repo only, target only, both, or neither.

- **`dotling remove` improvements**
  - Now restores tracked files to their original paths (decrypting if needed) instead of deleting them.
  - Preserves local edits on target files that are already regular files/directories.

- **Development tooling**
  - Added `justfile` for common dev tasks (format, lint, test, build).
  - Updated CI workflows and Nix dev environment.

---

## v0.4.0

### Added

- **Bidirectional `sync`** — replaces the old `deploy` command. Syncs changes in both directions: repo → local and local → repo.
- **Recursive directory encryption** — `encrypt` and `decrypt` now handle entire directories.

### Changed

- `remove` always restores the original file and deletes the repo source. The `--purge` flag has been removed.

---

## v0.3.1

### Fixed

- Vault architecture now correctly uses the master secret via key encapsulation (`DOTLING-ENC-V2`).
- Absolute paths and `~`-relative paths are resolved correctly during config lookups.
- No longer attempts to encrypt or decrypt entire directories.

---

## v0.3.0

### Changed

- Simplified test assertions; switched to `tempfile` for test directory management.
- Applied consistent `rustfmt` style across all modules.
- Rewrote core modules, replaced the printer with a UI layer, and simplified the CLI command structure.

---

## v0.2.1

### Added

- **Automatic pull-back** — modified entries are pulled back during push. Added `--all` flag to the pull-back command.

---

## v0.2.0

### Added

- **Age-based encryption** — secure file encryption with key generation support.
- **Core dotfiles management** — CLI and project scaffolding for tracking and deploying dotfiles.
- **Git-based infrastructure** — repo-backed dotfile management with a CLI framework.
- **Project initialization** — Rust scaffolding and Nix development environment.

---

## v0.1.0

Initial release.
