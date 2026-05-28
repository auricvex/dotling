# Changelog

All notable changes to this project will be documented in this file.

## [0.6.1]

- **feat**: `dotling edit` — Encrypted Template Editing
  - Added `dotling edit <entry>` command to edit any tracked entry in `$EDITOR` without a manual decrypt/re-encrypt cycle.
  - **Encrypted entries** — dotling decrypts the `.enc` source to a secure temporary file, launches the editor, then automatically re-encrypts the modified content back into the repo on save.
  - **Plain, copy, and template entries** — the repo source file is opened directly.
  - Entry lookup accepts a source path, target path, or partial match — the same flexible query as other commands.
  - Editor is resolved in priority order: `$DOTLING_EDITOR` → `$VISUAL` → `$EDITOR` → `vim` → `nano`.
  - Run `dotling sync` after editing to push re-encrypted changes out to the deployed target.
- **fix**: Hook Retry Logic
  - Hook commands that exit with a non-zero status are now automatically retried up to **3 attempts** before the sync is aborted.
  - A warning is printed after each failed attempt with the attempt count so the failure is always visible.
  - Error message on final failure now reports the total number of attempts made.
  - Added unit tests (`test_run_hook_retries_on_failure`, `test_run_hook_succeeds_without_retry`) to verify retry behaviour.

## [0.6.0]

- **feat**: Dotfile Templating (`dotling add --template`, `dotling vars`)
  - Introduced opt-in templating for dotfiles using `.dtmpl`-suffixed source files stored in the repo. Template files are rendered on every `sync` — machine-specific values are substituted, and the rendered output is deployed.
  - **Template syntax** — `{{ var.key }}` for user-defined variables, `{{ dotling.hostname }}` / `dotling.username` / `dotling.os` / `dotling.arch` / `dotling.home` / `dotling.repo` for built-ins, `{{ env.VAR }}` for environment variables.
  - **Pipe filters** — `upper`, `lower`, `trim`, `quote`, `squote`, `default:fallback` (e.g. `{{ var.name | upper }}`).
  - **Whitespace control** — `{{- expr -}}` strips surrounding whitespace from the rendered output.
  - **Hard-error on unresolved variables** — `sync` and `add` both abort with a clear message and `dotling vars set` hints when a variable is missing.
  - **Variable store** — machine-local variables live in `~/.dotling/vars.toml` (never committed); shared non-sensitive defaults live in `[vars]` in `dotling.toml` (committed). Local values take priority.
  - **`dotling add <path> --template`** — validates syntax, copies the source into the repo as `<name>.dtmpl`, renders immediately, and deploys the rendered output. Combine with `--encrypt` to store the template ciphertext as `<name>.dtmpl.enc`.
  - **`dotling vars`** subcommand with seven actions:
    - `list` — show all resolved variables (built-ins, config defaults, local) with their source tags.
    - `set <key> <value>` — save a machine-local variable.
    - `get <key>` — print the resolved value and source of a single variable.
    - `unset <key>` — remove a variable from the local store.
    - `check` — validate all `.dtmpl` entries and report unresolved variables with fix hints.
    - `import <path>` — bulk-import variables from a `.toml` `[vars]` section or a `.env` file.
    - `export` — print local variables as TOML for migrating to a new machine.
  - **Bootstrap prompt** — on first `sync` on a new machine, dotling detects missing variables across all template entries and interactively prompts for their values before syncing.
  - **Fingerprint-aware sync** — template entries skip push/pull conflict logic entirely; the rendered content is hashed and only re-written when the output would actually change.
  - **`dotling status`** — template entries display a distinct `📄` badge and fingerprint-based drift detection.
  - **`dotling doctor`** — reports unresolved variables per template entry with `dotling vars set` hints, and warns if any `[vars]` default looks like a real value (email, long token, matches current username).
  - **Security** — sensitive per-machine values go in `~/.dotling/vars.toml` (never committed); shared non-sensitive defaults go in `[vars]` in `dotling.toml`. Sensitive templates can be encrypted: the pipeline is `vault decrypt → render → deploy`.

## [0.5.0]

- **feat**: Backup Subsystem (`dotling backup`)
  - Added a dedicated backup module to handle automated file and directory backups before overwriting local files.
  - Organises backups under `~/.dotling/backups/<unix-seconds>/<repo-relative-source-path>` for safe chronological tracking.
  - Implemented `dotling backup list` command to display all recorded backup sessions.
  - Implemented `dotling backup clean [--keep-last N] [--older-than DAYS]` to prune backup sessions, defaulting to keeping the 10 most recent sessions.
  - Added a `--backup` flag to the `sync` command to always force a local backup before any overwriting push.
- **feat**: Lifecycle Hooks Support
  - Added support for custom before/after sync commands at the repository (global) level or per-entry level in `dotling.toml`.
  - Global hooks configured under `[hooks]` block: `init`, `before`, `after`.
  - Entry-specific hooks configured under entry properties: `before`, `after`.
  - Created interactive CLI verification prompts for untrusted hooks, allowing users to run once (`[y]es`), skip (`[n]o`), skip all (`[s]kip all`), or always trust (`[a]lways`, which stores a Blake2s-256 hash of the command string in `~/.dotling/state/trusted_hooks`).
  - Rich environment context passed to hook processes (`DOTLING_HOOK_TYPE`, `DOTLING_REPO_ROOT`, `DOTLING_DRY_RUN`, `DOTLING_ENTRY_SOURCE`, `DOTLING_ENTRY_TARGET`, `DOTLING_ENTRY_ACTION`).
  - Added CLI flags `--allow-hooks`/`--no-hooks` and environment variables `DOTLING_ALLOW_HOOKS=1`/`DOTLING_NO_HOOKS=1` for automated environments.
- **feat**: Line-Level Three-Way Merge
  - Implemented interactive `[m]erge` option for copy-mode plain files during conflict resolution.
  - Utilizes standard line-granular three-way merge against stored last-in-sync baselines (`~/.dotling/snapshots/<source>`).
  - Inserts git-style conflict markers (`<<<<<<< repo`, `=======`, `>>>>>>> actual`) for overlapping changes, and cleanly auto-merges non-overlapping changes.
  - Mirrors merge outcomes back to both the repository source and local target.
- **feat**: Sync Fingerprints for Encrypted and Copy Entries
  - Introduced `~/.dotling/fingerprints.toml` to record Blake2s-256 hashes of `.enc` ciphertext and target plaintext after sync.
  - Allows `status` and `sync` commands to quickly audit encrypted entries without requiring a vault password.
  - Tracks both repo source and target file hashes for plain copy-mode entries to deterministically detect if modifications happened on the repository (`RepoOnly`), the local target (`ActualOnly`), both sides (`Both`), or neither (`Neither`).
- **feat**: Refactored `remove` Command
  - Refactored `dotling remove` to restore tracked files or folders recursively (decrypting if encrypted) to their original paths instead of deleting them.
  - Safely preserves any local edits on target files if they are already regular files/directories instead of symlinks.
  - Deletes the original source files/folders from the repository and removes the tracking config entry.
- **feat**: Streamlined Development and CI
  - Introduced a comprehensive `justfile` for running common development tasks (formatting, clippy, testing, building).
  - Updated CI workflows and Nix development environment details.

## [0.4.0]
- **feat**: replace `deploy` command with bidirectional `sync` (repo → actual and actual → repo)
- **feat**: implement recursive directory encryption and decryption in `encrypt`/`decrypt` commands
- **fix**: make `remove` always purge the repo source file and restore the original — remove the `--purge` flag
- **chore**: bump project version to 0.4.0

## [0.3.1]
- **fix**: refactor Vault architecture to correctly utilize the master secret via Key Encapsulation (`DOTLING-ENC-V2`)
- **fix**: resolve absolute paths and home directory relative paths during config lookups
- **fix**: prevent attempting to encrypt or decrypt entire directories
- **chore**: bump project version to 0.3.1

## [0.3.0]
- **refactor**: simplify test assertions and use tempfile for robust test directory management
- **refactor**: apply consistent rustfmt code style and formatting across all modules
- **chore**: ignore result
- **refactor**: rewrite core modules, replace printer with UI, and simplify CLI command structure

## [0.2.1]
- **chore**: bump project version to 0.2.1
- **feat**: implement automatic pull-back of modified entries during push and add `--all` flag to pull-back command

## [0.2.0]
- **chore**: bump project version to 0.2.0
- **docs**: add documentation for native age-based encryption and new keygen workflow
- **feat**: implement secure file encryption using age and add key generation support
- **refactor**: reformat code and update Platform default instantiation for consistency
- **feat**: implement core dotfiles management CLI and project scaffolding
- **feat**: implement core git-based dotfile management infrastructure and CLI framework
- **feat**: initialize project with Rust scaffolding and Nix development environment configuration
