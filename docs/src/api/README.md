# API Documentation

This section documents the public Rust API of the `dotling` crate, similar to rustdoc output. dotling is primarily a CLI tool, but its modules can be used as a library for building custom dotfile management workflows.

## Module overview

| Module | Description |
|---|---|
| [core](./core.md) | Error types, filesystem helpers, path mapping, platform detection, state store |
| [config](./config.md) | Configuration data model, TOML parser, template engine, variable store |
| [crypto](./crypto.md) | ChaCha20-Poly1305 encryption, Argon2id key derivation, vault management |
| [sync](./sync.md) | Entry deployment, fingerprint tracking, lifecycle hooks, three-way merge |

## Re-exports

The crate root re-exports key types:

```rust
pub use error::{Error, Result};
pub use core::{error, fs, path, platform, store};
pub use config::{template, vars};
pub use sync::{deploy, fingerprint, hooks, merge};
```
