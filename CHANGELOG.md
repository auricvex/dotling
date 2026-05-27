# Changelog

All notable changes to this project will be documented in this file.

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
