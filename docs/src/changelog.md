# Changelog

All notable changes to dotling are documented here.

Each release follows [Keep a Changelog](https://keepachangelog.com/) conventions: **Added**, **Changed**, **Fixed**, **Removed**.

---

## v0.8.0

### Changed

- **Vault export/import — single encrypted bundle format** — `vault export` now writes a single encrypted bundle file instead of copying raw vault files. `vault import` decrypts the bundle with your password and verifies the identity before writing. Old directory-based bundles are no longer compatible; re-export from your source machine after upgrading.
- **Encryption/decryption refactored** — `encrypt` and `decrypt` now operate in-place on tracked entries, consolidating file handling into shared helpers. Directory encryption uses the same pipeline as single-file encryption.
- **Fingerprint tracking for template entries** — Templates now participate in the fingerprint store, enabling `status` and `sync --dry-run` to detect template drift without decrypting.

### Removed

- **Backup system** — Removed the `dotling backup` command, the `--backup` flag on `sync`, and all automatic backup-before-overwrite behavior.

### Added

- Comprehensive roundtrip tests for encryption and decryption, and a full template sync lifecycle test suite.

---

## v0.7.0

### Added

- **Shell completions** (`dotling completions <SHELL>`) — Generate tab-completion scripts for bash, zsh, fish, elvish, and powershell.

---

## v0.6.2

### Changed

- **Module restructuring** — Reorganized the codebase into a layered architecture with `core/`, `config/`, and `sync/` top-level modules.

### Fixed

- Minor formatting cleanup in the sync command hook error message.

---

## v0.6.1

### Added

- **`dotling edit <entry>`** — Open any tracked file in your editor without manually decrypting and re-encrypting.

### Fixed

- **Hook retry logic** — Hooks that fail are now retried up to 3 times before the sync is aborted.

---

## v0.6.0

### Added

- **Dotfile templating** (`dotling add --template`, `dotling vars`) — Render machine-specific values into your dotfiles automatically.
- Template syntax with `{{ var.key }}`, built-in variables, environment variables, pipe filters, and whitespace control.
- `dotling vars` subcommand with seven actions: list, set, get, unset, check, import, export.
- Bootstrap prompt for missing variables on new machines.

---

## v0.5.0

### Added

- **Lifecycle hooks** — Run custom commands before/after sync, globally or per-entry, with trust verification.
- **Line-level three-way merge** — Interactive merge option for copy-mode files during conflict resolution.
- **Sync fingerprints** — Blake2s-256 content hash tracking for encrypted and copy-mode entries.
- **`dotling remove` improvements** — Now restores tracked files to their original paths.

---

## v0.4.0

### Added

- **Bidirectional `sync`** — Replaces the old `deploy` command. Syncs changes in both directions.
- **Recursive directory encryption** — `encrypt` and `decrypt` now handle entire directories.

### Changed

- `remove` always restores the original file and deletes the repo source.

---

## v0.3.1

### Fixed

- Vault architecture now correctly uses the master secret via key encapsulation.
- Absolute paths and `~`-relative paths are resolved correctly during config lookups.

---

## v0.3.0

### Changed

- Rewrote core modules, replaced the printer with a UI layer, and simplified the CLI command structure.

---

## v0.2.1

### Added

- **Automatic pull-back** — modified entries are pulled back during push.

---

## v0.2.0

### Added

- Age-based encryption with key generation support.
- Core dotfiles management CLI and project scaffolding.

---

## v0.1.0

Initial release.
