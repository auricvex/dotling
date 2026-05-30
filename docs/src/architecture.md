# Architecture

dotling is structured as a layered Rust application with a command pattern. Each CLI subcommand maps to a module in `src/commands/`, and core logic is organized into four top-level modules.

## Module hierarchy

```
dotling
├── core/                  Foundational utilities
│   ├── error              Error enum and Result type alias
│   ├── fs                 Filesystem helpers (walk, copy, symlink, atomic write)
│   ├── path               Path mapping, tilde expansion, category rules
│   ├── platform           OS detection, platform matching
│   └── store              Global state at ~/.dotling/
│
├── config/                Data model and rendering
│   ├── mod.rs             Config, Entry, Settings, Hooks, TOML parser/serializer
│   ├── template           Template engine: {{ var.x }}, {{ dotling.x }}, {{ env.X }}
│   └── vars               Machine-local variable store
│
├── crypto/                Encryption
│   ├── mod.rs             ChaCha20-Poly1305 encryption, Argon2id key derivation
│   └── vault              Vault management: init, unlock, export, import
│
├── sync/                  Sync and deployment
│   ├── mod.rs             Re-exports submodules
│   ├── deploy             Entry deployment: symlink/copy, state checking
│   ├── fingerprint        Blake2s-256 content hashing for change detection
│   ├── hooks              Lifecycle hooks with trust verification
│   └── merge              Line-level three-way merge using LCS diff
│
├── cli.rs                 clap derive definitions for all CLI args/subcommands
├── commands/              Command handlers (one module per subcommand)
│   ├── init, add, remove, sync, status, edit
│   ├── encrypt, vault, doctor, vars, completions
│
├── ui.rs                  Terminal UI: colors, prompts, diff display
└── main.rs                Entry point: parse CLI, dispatch, handle errors
```

## Data flow

```
CLI input (clap)
  → main.rs (parse args, dispatch)
    → commands/<subcommand>::run()
      → core/store (load repo root from ~/.dotling/state.toml)
      → config/mod (load dotling.toml)
      → sync engine
        → sync/deploy (create/fix symlinks, copy files)
        → sync/fingerprint (record/check content hashes)
        → sync/hooks (execute lifecycle hooks)
        → sync/merge (three-way merge for conflicts)
        → crypto (encrypt/decrypt as needed)
        → config/template (render templates)
      → ui (display results)
```

## Data model

### `Config` (`dotling.toml`)

The central configuration file at the repo root. Contains:

- `Settings` — global defaults (deployment method)
- `Vec<Entry>` — all tracked files/directories
- `Hooks` — global lifecycle hooks
- `Vec<(String, String)>` — shared variable defaults

The TOML parser and serializer are hand-rolled (no serde dependency).

### `Entry`

Each tracked file or directory:

| Field | Type | Description |
|---|---|---|
| `source` | `String` | Repo-relative path |
| `target` | `String` | Deploy target path with `~` |
| `method` | `Option<DeployMethod>` | `Symlink` or `Copy` override |
| `encrypted` | `bool` | Whether the entry is encrypted |
| `directory` | `bool` | Whether the entry is a directory |
| `template` | `bool` | Whether the entry is a template |
| `os` | `Option<String>` | Platform restriction |
| `permissions` | `Option<u32>` | Octal permissions |
| `before` | `Option<String>` | Pre-sync hook |
| `after` | `Option<String>` | Post-sync hook |

### `FingerprintStore` (`~/.dotling/fingerprints.toml`)

Maps source paths to `EntryFingerprint` records with three Blake2s-256 hashes: `enc_hash` (ciphertext), `target_hash` (deployed file), `source_hash` (repo source).

### `VarStore` (`~/.dotling/vars.toml`)

Ordered list of `(key, value)` pairs for machine-local template variables.

## Dependencies

| Crate | Purpose |
|---|---|
| `clap` | CLI argument parsing with derive macros |
| `clap_complete` | Shell completion generation |
| `chacha20poly1305` | AEAD encryption |
| `argon2` | Password-based key derivation |
| `blake2` | Content hashing (fingerprints, hook trust) |
| `rand` | Cryptographic random number generation |
| `base64` | Binary-to-text encoding for encrypted files |

No serde. No async runtime. Minimal by design.

## Testing

Tests are inline `#[cfg(test)] mod tests` blocks across 15 files (94 tests total). Uses `tempfile` for temporary directories and `serial_test` for tests that mutate environment variables.
