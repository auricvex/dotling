# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Dotling is a dotfiles manager written in Rust. It tracks config files from `~` into a git-backed repo, deploying symlinks (or copies) back to their original locations. Supports encryption (ChaCha20-Poly1305), template rendering with variables, bidirectional sync with conflict resolution, and lifecycle hooks.

## Build & Development Commands

All commands use `just` (see `justfile`). Inside Nix: `nix develop --command just <recipe>`.

| Task | Command |
|---|---|
| Build | `just build` |
| Release build | `just release` |
| Run | `just run -- <args>` |
| Test | `just test` |
| Run single test | `cargo test --all-features -- <test_name>` |
| Type check | `just check` |
| Lint (clippy) | `just clippy` |
| Format | `just fmt` |
| Format check | `just fmt-check` |
| Full local CI | `just ci` (runs fmt-check, clippy, test, deny, audit) |

## Toolchain

- **Rust nightly** (pinned in `rust-toolchain.toml`)
- **Edition 2024**, MSRV 1.85
- **Nix dev shell** via `flake.nix` with direnv integration (`.envrc`)

## Architecture

**Layered module structure with command pattern.** Each CLI subcommand maps to a module in `src/commands/`. Core logic is organized into four top-level modules: `core`, `config`, `crypto`, and `sync`.

**Data flow:** CLI (clap via `cli.rs`) -> global state (`~/.dotling/state.toml` via `core/store.rs`) -> config (`<repo>/dotling.toml` via `config/mod.rs`) -> command handler -> core modules -> UI output (`ui.rs`).

### Module Layout (`src/`)

**`core/`** — Foundational utilities:
- `error.rs` — Unified `Error` enum: `Io`, `Config`, `Crypto`, `Deploy`, `Vault`, `Template`, `User`
- `fs.rs` — Filesystem helpers
- `path.rs` — Path mapping: `~/.config/nvim` -> `config/nvim`, category rules for shell/git/vim/tmux/ssh/gnupg
- `platform.rs` — Platform detection
- `store.rs` — Global state at `~/.dotling/`: repo root, paths to fingerprints/vars

**`config/`** — Data model and rendering:
- `mod.rs` — Data model (`Entry`, `Config`, `Settings`, `Hooks`) and hand-rolled TOML parser/serializer. No serde.
- `template.rs` — Template engine: `{{ var.key }}`, `{{ dotling.hostname }}`, `{{ env.VAR }}` with pipe filters (`upper`, `lower`, `trim`, `quote`, `squote`, `default`) and whitespace control `{{- -}}`
- `vars.rs` — Machine-local variable store at `~/.dotling/vars.toml`

**`crypto/`** — Encryption:
- `mod.rs` — ChaCha20-Poly1305 encryption, Argon2id key derivation
- `vault.rs` — Vault at `~/.dotling/vault/`: init, unlock, export, import, change-password

**`sync/`** — Sync and deployment:
- `mod.rs` — Sync orchestration
- `deploy.rs` — Entry deployment: symlink/copy creation, encrypted deployment, state checking (`EntryState`)
- `fingerprint.rs` — Blake2s-256 content hashing for sync-state tracking of encrypted/copy-mode entries
- `hooks.rs` — Lifecycle hook execution with trust verification (Blake2s hash of command string), 3-attempt retry
- `merge.rs` — Line-level three-way merge using LCS diff, produces git-style conflict markers

**Top-level modules:**
- `cli.rs` — clap derive definitions for all CLI args/subcommands
- `ui.rs` — Terminal UI: ANSI colors (respects `NO_COLOR`), interactive prompts, password input, diff display
- `main.rs` — Thin CLI entry point: parses args, dispatches to commands, handles errors

### Command Modules (`src/commands/`)

`init`, `add`, `remove`, `sync`, `status`, `edit`, `encrypt`, `vault`, `doctor`, `vars`, `completions`

`Encrypt` and `Decrypt` are separate CLI subcommands but both handled by `commands/encrypt.rs`. `Vault` and `Vars` each have nested sub-action enums.

## Code Style & Linting

**Formatting** (`rustfmt.toml`): 100-char width, 4-space indent, nightly features. Imports grouped as `std -> external -> crate`, sorted alphabetically. Trailing commas on multiline. Run `just fmt` before committing.

**Clippy** (`clippy.toml`): Warnings treated as errors (`-D warnings`). Key constraints:
- Cognitive complexity threshold: 20
- Function line limit: 80
- Function arg limit: 6
- **Banned methods:** `std::thread::sleep`, `std::process::exit`, `std::env::temp_dir`
- **Banned types:** `std::sync::Mutex`, `std::sync::RwLock`, `std::sync::Condvar`
- **Banned macros:** `dbg!`, `todo!`, `unimplemented!`

**Lint levels** are set in `Cargo.toml` under `[lints.clippy]`.

## Testing

Tests are inline `#[cfg(test)] mod tests` blocks (no separate `tests/` directory). Use `tempfile` for temp dirs. 94 tests across 15 files, focused on core logic (config parsing, template rendering, merge, fingerprinting, crypto roundtrips, hook trust, shell completions).

## Dependencies

Minimal: `clap` (CLI), `clap_complete` (shell completions), `chacha20poly1305` + `argon2` + `blake2` + `rand` + `base64` (crypto). Dev: `tempfile`. No serde, no async runtime.
