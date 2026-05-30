# core

Foundational utilities for error handling, filesystem operations, path mapping, platform detection, and global state management.

## `core::error`

### `Result<T>`

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

Convenience alias for `std::result::Result<T, dotling::Error>`.

### `Error`

```rust
pub enum Error {
    Io {
        path: PathBuf,
        operation: &'static str,
        source: io::Error,
    },
    Config {
        message: String,
        line: Option<usize>,
    },
    Crypto(String),
    Deploy {
        entry: String,
        message: String,
    },
    Vault(String),
    Template {
        source: String,
        message: String,
    },
    User(String),
}
```

Unified error type for all dotling operations.

| Variant | Usage |
|---|---|
| `Io` | Filesystem errors with path and operation context |
| `Config` | Configuration parsing errors with optional line number |
| `Crypto` | Encryption/decryption failures |
| `Deploy` | Deployment errors with entry name |
| `Vault` | Vault operation failures |
| `Template` | Template rendering errors with source file |
| `User` | User-facing errors (e.g., cancelled prompts) |

### `Error::io()`

```rust
pub fn io(path: impl Into<PathBuf>, operation: &'static str, source: io::Error) -> Self
```

Constructs an `Error::Io` variant.

---

## `core::fs`

Filesystem helper functions. All operations are atomic where possible (write to temp, then rename).

### `walk_dir()`

```rust
pub fn walk_dir(root: &Path, include_hidden: bool) -> Result<Vec<PathBuf>>
```

Recursively walks a directory and returns all file paths. When `include_hidden` is false, skips files and directories starting with `.`.

### `copy_file()`

```rust
pub fn copy_file(src: &Path, dst: &Path) -> Result<()>
```

Copies a file from `src` to `dst`, creating parent directories as needed.

### `create_symlink()`

```rust
pub fn create_symlink(target: &Path, link: &Path) -> Result<()>
```

Creates a symbolic link at `link` pointing to `target`. Creates parent directories as needed.

### `remove_symlink()`

```rust
pub fn remove_symlink(path: &Path) -> Result<()>
```

Removes a symbolic link without following it.

### `atomic_write()`

```rust
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()>
```

Writes data to a file atomically by writing to a temporary file in the same directory, then renaming.

### `is_symlink()`

```rust
pub fn is_symlink(path: &Path) -> bool
```

Returns true if the path is a symbolic link.

### `read_link()`

```rust
pub fn read_link(path: &Path) -> Result<PathBuf>
```

Reads the target of a symbolic link.

### `files_identical()`

```rust
pub fn files_identical(a: &Path, b: &Path) -> Result<bool>
```

Compares two files byte-by-byte and returns true if they are identical.

### `cleanup_empty_parents()`

```rust
pub fn cleanup_empty_parents(path: &Path, stop_at: &Path) -> Result<()>
```

Removes empty parent directories up to (but not including) `stop_at`.

### `set_permissions()` (Unix only)

```rust
pub fn set_permissions(path: &Path, mode: u32) -> Result<()>
```

Sets Unix file permissions using an octal mode (e.g., `0o600`).

### `get_permissions()` (Unix only)

```rust
pub fn get_permissions(path: &Path) -> Result<Option<u32>>
```

Returns the Unix file permissions as an octal mode, or `None` on non-Unix platforms.

---

## `core::path`

Path mapping and tilde expansion utilities.

### `home_dir()`

```rust
pub fn home_dir() -> Result<PathBuf>
```

Returns the user's home directory.

### `expand_tilde()`

```rust
pub fn expand_tilde(path: &Path) -> Result<PathBuf>
```

Expands `~` at the start of a path to the user's home directory.

### `collapse_tilde()`

```rust
pub fn collapse_tilde(path: &Path) -> PathBuf
```

Replaces the home directory prefix with `~` in a path.

### `relative_to()`

```rust
pub fn relative_to(target: &Path, base: &Path) -> Option<PathBuf>
```

Returns `target` as a relative path from `base`, or `None` if not possible.

### `map_to_repo()`

```rust
pub fn map_to_repo(home_path: &Path) -> Result<PathBuf>
```

Maps a home directory path to its repo-relative path using category rules:

| Pattern | Repo path |
|---|---|
| `.config/*` | `config/*` |
| Shell files (`.zshrc`, `.bashrc`, etc.) | `shell/` |
| Git files (`.gitconfig`, etc.) | `git/` |
| Vim files (`.vimrc`, `.vim/`) | `vim/` |
| Tmux files (`.tmux.conf`, etc.) | `tmux/` |
| SSH files (`.ssh/`) | `ssh/` |
| GnuPG files (`.gnupg/`) | `gnupg/` |
| Everything else | `home/` |

### `resolve()`

```rust
pub fn resolve(path: &Path) -> Result<PathBuf>
```

Resolves a path to an absolute path, expanding `~` and resolving `.` and `..`.

---

## `core::platform`

Platform detection and matching.

### `Platform`

```rust
pub enum Platform {
    Linux,
    Macos,
    Windows,
}
```

### `Platform::current()`

```rust
pub fn current() -> Self
```

Returns the current platform at compile time.

### `Platform::parse()`

```rust
pub fn parse(s: &str) -> Option<Self>
```

Parses a platform string. Accepts: `"linux"`, `"macos"` / `"darwin"`, `"windows"` / `"win"`.

### `Platform::as_str()`

```rust
pub fn as_str(self) -> &'static str
```

Returns the platform as a lowercase string: `"linux"`, `"macos"`, or `"windows"`.

### `should_deploy()`

```rust
pub fn should_deploy(os: Option<&str>) -> bool
```

Returns true if an entry with the given `os` tag should be deployed on the current platform. `None` or `"all"` means deploy everywhere.

---

## `core::store`

Global state management at `~/.dotling/`.

### `state_dir()`

```rust
pub fn state_dir() -> Result<PathBuf>
```

Returns the path to `~/.dotling/`. Creates it if it doesn't exist.

### `fingerprint_path()`

```rust
pub fn fingerprint_path() -> Result<PathBuf>
```

Returns the path to `~/.dotling/fingerprints.toml`.

### `vars_path()`

```rust
pub fn vars_path() -> Result<PathBuf>
```

Returns the path to `~/.dotling/vars.toml`.

### `snapshot_dir()`

```rust
pub fn snapshot_dir() -> Result<PathBuf>
```

Returns the path to `~/.dotling/snapshots/`. Creates it if it doesn't exist.

### `snapshot_path()`

```rust
pub fn snapshot_path(source: &str) -> Result<PathBuf>
```

Returns the path to a specific snapshot file for the given source entry.

### `get_repo_root()`

```rust
pub fn get_repo_root() -> Result<Option<PathBuf>>
```

Returns the registered repo root from `~/.dotling/state.toml`, or `None` if not set.

### `set_repo_root()`

```rust
pub fn set_repo_root(repo_root: &Path) -> Result<()>
```

Registers the repo root in `~/.dotling/state.toml`.

### `require_repo_root()`

```rust
pub fn require_repo_root() -> Result<PathBuf>
```

Returns the repo root, or returns an error if not registered.

### `config_path()`

```rust
pub fn config_path(repo_root: &Path) -> PathBuf
```

Returns the path to `dotling.toml` within the given repo root.
