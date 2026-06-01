# sync

Entry deployment, fingerprint tracking, lifecycle hooks, and three-way merge.

## `sync::deploy`

### `EntryState`

```rust
pub enum EntryState {
    Deployed,
    Modified,
    Missing,
    Broken,
    Conflict,
}
```

The current state of a tracked entry.

| State | Meaning |
|---|---|
| `Deployed` | Entry is correctly deployed and in sync |
| `Modified` | Entry has local changes that differ from the repo |
| `Missing` | Target file is missing |
| `Broken` | Symlink points to a non-existent target |
| `Conflict` | Both repo and local file have changed |

### `check_state()`

```rust
pub fn check_state(
    entry: &Entry,
    repo_root: &Path,
    default_method: DeployMethod,
) -> EntryState
```

Checks the current deployment state of an entry.

### `deploy_entry()`

```rust
pub fn deploy_entry(
    entry: &Entry,
    repo_root: &Path,
    default_method: DeployMethod,
    force: bool,
) -> Result<()>
```

Deploys an entry: creates a symlink or copies the file from the repo to the target path. When `force` is false, refuses to overwrite unmanaged files.

### `deploy_encrypted()`

```rust
pub fn deploy_encrypted(
    entry: &Entry,
    repo_root: &Path,
    password: &str,
) -> Result<()>
```

Deploys an encrypted entry: decrypts the repo file and writes the plaintext to the target path.

---

## `sync::fingerprint`

### `EntryFingerprint`

```rust
pub struct EntryFingerprint {
    pub enc_hash: String,
    pub target_hash: String,
    pub source_hash: String,
}
```

Content hashes for an entry at the time of last sync.

| Field | Description |
|---|---|
| `enc_hash` | Blake2s-256 hash of the encrypted file (if applicable) |
| `target_hash` | Blake2s-256 hash of the deployed target file |
| `source_hash` | Blake2s-256 hash of the repo source file |

### `WhichSide`

```rust
pub enum WhichSide {
    Unknown,
    Neither,
    RepoOnly,
    ActualOnly,
    Both,
}
```

Indicates which side of a sync pair has changed.

| Variant | Meaning |
|---|---|
| `Unknown` | No fingerprint record exists |
| `Neither` | Both sides match their fingerprints |
| `RepoOnly` | Only the repo source has changed |
| `ActualOnly` | Only the deployed target has changed |
| `Both` | Both sides have changed (conflict) |

### `FingerprintStore`

```rust
pub struct FingerprintStore { /* private */ }
```

Store for entry fingerprints, backed by `~/.dotling/fingerprints.toml`.

### `FingerprintStore::load()`

```rust
pub fn load(path: PathBuf) -> Self
```

Loads the fingerprint store from the given path.

### `FingerprintStore::has_record()`

```rust
pub fn has_record(&self, source: &str) -> bool
```

Returns `true` if a fingerprint exists for the given source.

### `FingerprintStore::record()`

```rust
pub fn record(
    &mut self,
    source: &str,
    enc_path: &Path,
    target_path: &Path,
) -> Result<()>
```

Records fingerprints for an encrypted entry.

### `FingerprintStore::record_plain()`

```rust
pub fn record_plain(
    &mut self,
    source: &str,
    source_path: &Path,
    target_path: &Path,
) -> Result<()>
```

Records fingerprints for a plain (non-encrypted) entry.

### `FingerprintStore::is_in_sync()`

```rust
pub fn is_in_sync(
    &self,
    source: &str,
    enc_path: &Path,
    target_path: &Path,
) -> Option<bool>
```

Returns `Some(true)` if the entry is in sync, `Some(false)` if not, `None` if no record exists.

### `FingerprintStore::who_changed()`

```rust
pub fn who_changed(
    &self,
    source: &str,
    source_path: &Path,
    target_path: &Path,
) -> WhichSide
```

Determines which side has changed since the last sync.

### `FingerprintStore::save()`

```rust
pub fn save(&self) -> Result<()>
```

Saves the fingerprint store to disk.

### `hash_path()`

```rust
pub fn hash_path(path: &Path) -> Result<String>
```

Computes a Blake2s-256 hash of a file or directory (recursive).

### `hash_file()`

```rust
pub fn hash_file(path: &Path) -> Result<String>
```

Computes a Blake2s-256 hash of a single file.

---

## `sync::hooks`

### `HookSession`

```rust
pub struct HookSession { /* private */ }
```

Manages hook execution with trust verification.

### `HookSession::new()`

```rust
pub fn new(allow_hooks: bool, no_hooks: bool) -> Self
```

Creates a new hook session. `allow_hooks` auto-trusts all hooks. `no_hooks` disables all hook execution.

### `HookSession::verify_and_allow()`

```rust
pub fn verify_and_allow(
    &mut self,
    command: &str,
    hook_type: &str,
    no_interactive: bool,
) -> Result<bool>
```

Checks if a hook command is trusted. If not, prompts the user for verification. Returns `true` if the hook should run.

### `HookSession::run_hook()`

```rust
pub fn run_hook(
    &mut self,
    command: &str,
    hook_type: &str,
    repo_root: &Path,
    dry_run: bool,
    no_interactive: bool,
    entry: Option<&Entry>,
    entry_action: Option<&str>,
) -> Result<()>
```

Executes a hook command. Verifies trust first, sets environment variables, and retries up to 3 times on failure.

---

## `sync::merge`

### `MergeResult`

```rust
pub struct MergeResult {
    pub content: String,
    pub has_conflicts: bool,
    pub conflict_count: usize,
}
```

Result of a three-way merge.

### `three_way_merge()`

```rust
pub fn three_way_merge(
    base: &str,
    ours: &str,
    theirs: &str,
    ours_label: &str,
    theirs_label: &str,
) -> MergeResult
```

Performs a line-level three-way merge using LCS (Longest Common Subsequence) diff. Non-overlapping changes are auto-merged. Overlapping conflicts are marked with git-style conflict markers:

```text
<<<<<<< repo
repo version content
=======
actual local content
>>>>>>> actual
```
