# Contributing to Dotling

Thanks for your interest in contributing! This document covers the practical details of getting changes merged.

## Getting Started

1. Fork the repository and clone your fork.
2. Enter the dev shell (requires [Nix](https://nixos.org/download.html) with flakes enabled):
   ```sh
   nix develop
   ```
   Or install Rust nightly manually (the pinned version is in `rust-toolchain.toml`).

3. Build and run tests:
   ```sh
   just ci
   ```

## Development Workflow

### Branching

- Create a feature branch from `main`:
  ```sh
  git checkout -b my-feature
  ```

### Code Style

- Format: `just fmt` (100-char width, 4-space indent, nightly rustfmt)
- Lint: `just clippy` (warnings are errors, cognitive complexity cap of 20)
- Keep functions under 80 lines and 6 arguments where possible.
- Run `just ci` before pushing to catch issues early.

### Commit Messages

Use clear, descriptive commit messages. Prefix with a category when it helps:

- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation only
- `refactor:` — code restructuring, no behavior change
- `test:` — adding or updating tests
- `chore:` — build, CI, dependency changes

### Tests

- Tests live inline as `#[cfg(test)] mod tests` blocks (no separate `tests/` directory).
- Use `tempfile` for temporary directories.
- Run a single test: `cargo test --all-features -- <test_name>`

### Opening a Pull Request

1. Push your branch and open a PR against `main`.
2. Fill in the PR template — describe what changed and why.
3. CI must pass (clippy, fmt, tests, deny, audit).
4. A maintainer will review. Address feedback with additional commits (avoid force-pushing during review).

## Reporting Issues

Use the [issue tracker](https://github.com/auricvex/dotling/issues) with the provided templates:

- **Bug Report** — something is broken or behaves unexpectedly.
- **Feature Request** — an idea for new functionality.

Include reproduction steps, expected vs actual behavior, and your platform/OS when filing bugs.

## Security

**Do not open public issues for security vulnerabilities.** See [SECURITY.md](SECURITY.md) for responsible disclosure instructions.

## License

By contributing, you agree that your contributions will be dual-licensed under the [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE-2.0) licenses.
