# Contributing

Contributions are welcome! See [CONTRIBUTING.md](https://github.com/auricvex/dotling/blob/main/CONTRIBUTING.md) for guidelines, development setup, and PR workflow.

## Quick reference

All development commands use `just`. Inside a Nix environment, prefix with `nix develop --command`.

| Task | Command |
|---|---|
| Build | `just build` |
| Release build | `just release` |
| Run | `just run -- <args>` |
| Test | `just test` |
| Type check | `just check` |
| Lint (clippy) | `just clippy` |
| Format | `just fmt` |
| Format check | `just fmt-check` |
| Full local CI | `just ci` |

## Toolchain

- **Rust nightly** (pinned in `rust-toolchain.toml`)
- **Edition 2024**, MSRV 1.85
- **Nix dev shell** via `flake.nix` with direnv integration

## Code style

- 100-char width, 4-space indent
- Imports grouped as `std -> external -> crate`, sorted alphabetically
- Trailing commas on multiline
- Run `just fmt` before committing

## Linting

Clippy warnings are treated as errors (`-D warnings`). Key constraints:

- Cognitive complexity threshold: 20
- Function line limit: 80
- Function arg limit: 6
- Banned: `std::thread::sleep`, `std::process::exit`, `std::env::temp_dir`, `dbg!`, `todo!`, `unimplemented!`
