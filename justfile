# justfile for dotling (Rust-based dotfiles manager)

set shell := ["bash", "-uc"]

# ------------------------------------------------------------------------------
# Default Recipe
# ------------------------------------------------------------------------------

# List all available recipes
default:
    @just --list

# ------------------------------------------------------------------------------
# Development & Build
# ------------------------------------------------------------------------------

# Compile the project in debug mode
build:
    cargo build

# Compile the project in release mode
release:
    cargo build --release

# Run the CLI with optional arguments (e.g. `just run -- --help`)
run *args:
    cargo run --all-features -- {{ args }}

# ------------------------------------------------------------------------------
# Testing & Verification
# ------------------------------------------------------------------------------

# Run the test suite with all features enabled
test *args:
    cargo test --all-features -- {{ args }}

# Run cargo check across all targets and features
check:
    cargo check --all-targets --all-features

# Run Clippy lints (warnings treated as errors to match CI)
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# ------------------------------------------------------------------------------
# Code Quality & Formatting
# ------------------------------------------------------------------------------

# Format the codebase using rustfmt
fmt:
    cargo fmt

# Check codebase formatting without modifying files (CI check)
fmt-check:
    cargo fmt --check

# ------------------------------------------------------------------------------
# Security & Licensing
# ------------------------------------------------------------------------------

# Check dependency licenses and bans using cargo-deny
deny:
    cargo deny check

# Audit dependencies for crates.io security advisories using cargo-audit
audit:
    cargo audit

# ------------------------------------------------------------------------------
# Local CI & Housekeeping
# ------------------------------------------------------------------------------

# Run the full validation suite locally (clippy, fmt-check, test, deny, audit)
ci: fmt-check clippy test deny audit
    @echo "================================================="
    @echo "   All local CI checks passed successfully! 🎉  "
    @echo "================================================="

# Clean build artifacts
clean:
    cargo clean
