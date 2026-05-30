// ── Core foundation ──────────────────────────────────────────────

pub mod core;

// Re-export core submodules at crate root so existing paths resolve
pub use core::{error, fs, path, platform, store};

// ── Configuration & templating ───────────────────────────────────

pub mod config;
pub use config::{template, vars};

// ── Crypto ───────────────────────────────────────────────────────

pub mod crypto;

// ── Sync engine ──────────────────────────────────────────────────

pub mod sync;
pub use sync::{deploy, fingerprint, hooks, merge};

// ── CLI & command dispatch ───────────────────────────────────────

pub mod cli;
pub mod commands;
pub mod ui;

// ── Convenience re-exports ───────────────────────────────────────

pub use error::{Error, Result};
