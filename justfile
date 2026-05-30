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

# Build the documentation site
docs-build:
    mdbook build docs

# Serve the documentation locally with live reload
docs-serve:
    mdbook serve docs

# Clean build artifacts
clean:
    cargo clean

# ------------------------------------------------------------------------------
# Shell Completions
# ------------------------------------------------------------------------------

# Generate bash completion script to stdout
completions-bash:
    cargo run -- completions bash

# Generate zsh completion script to stdout
completions-zsh:
    cargo run -- completions zsh

# Generate fish completion script to stdout
completions-fish:
    cargo run -- completions fish

# Install completions for the current shell (detected from $SHELL)
install-completions:
    #!/usr/bin/env bash
    set -euo pipefail
    shell=$$(basename "$$SHELL")
    case "$$shell" in
      bash)
        dir="$${BASH_COMPLETION_USER_DIR:-$$HOME/.local/share/bash-completion/completions}"
        mkdir -p "$$dir"
        cargo run -- completions bash > "$$dir/dotling"
        echo "Installed bash completions to $$dir/dotling"
        ;;
      zsh)
        dir="$$HOME/.zfunc"
        mkdir -p "$$dir"
        cargo run -- completions zsh > "$$dir/_dotling"
        echo "Installed zsh completions to $$dir/_dotling"
        echo "Ensure ~/.zshrc contains: fpath=(~/.zfunc \$$fpath) && autoload -Uz compinit && compinit"
        ;;
      fish)
        dir="$$HOME/.config/fish/completions"
        mkdir -p "$$dir"
        cargo run -- completions fish > "$$dir/dotling.fish"
        echo "Installed fish completions to $$dir/dotling.fish"
        ;;
      *)
        echo "Unsupported shell: $$shell. Use 'just completions-<shell>' and redirect manually."
        exit 1
        ;;
    esac
