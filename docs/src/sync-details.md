# Sync

`dotling sync` is the core command that keeps your repo and actual filesystem in sync. It handles symlinks, copies, encrypted entries, and templates — all in one pass.

## Sync direction

`dotling sync` decides the direction per entry:

| Entry type | Push (repo -> actual) | Pull (actual -> repo) |
|---|---|---|
| **Symlink** | Create/fix symlink | Never (symlink always reads repo) |
| **Copy** | Source newer or target missing | Target newer |
| **Encrypted** | `.enc` newer or target missing -> decrypt | Target newer -> re-encrypt |
| **Template** | Always renders and deploys | Never (template source is canonical) |

When both sides differ and timestamps are equal, dotling defaults to **repo wins** (push). Pass `--prefer-actual` to flip this.

## Sync process

For each entry, dotling:

1. Checks if the entry's OS matches the current platform (skips if not)
2. Determines the entry state: `Deployed`, `Modified`, `Missing`, `Broken`, or `Conflict`
3. Uses fingerprint comparison (for copy/encrypted) or mtime to decide direction
4. If both sides changed, prompts for conflict resolution (unless `--force` or `--no-interactive`)
5. Executes the sync action (push or pull)
6. Records fingerprints for future comparison
7. Runs entry-level hooks (before/after)

Global hooks (`before`/`after`) run at the very beginning and end of the sync session.

## Conflict resolution

When sync detects a conflict between the repository and your local target, you can choose:

| Option | Key | Description |
|---|---|---|
| Keep Local | `k` | Overwrite the repo with your local file (pulls to repo) |
| Use Repo | `r` | Overwrite the local file with the repo version (pushes to local) |
| Merge | `m` | Perform a three-way merge (see below) |
| Diff | `d` | Compare inline changes |
| Skip | `s` | Leave this entry unresolved and continue |

### Three-way merge

The merge option performs a line-level three-way merge using the last-in-sync snapshot as the base, combining modifications from both the repo (ours) and local target (theirs). Non-overlapping changes are cleanly auto-merged, while overlapping conflicts are highlighted with standard git conflict markers:

```text
<<<<<<< repo
repo version content
=======
actual local content
>>>>>>> actual
```

The merge outcome is written back to both the local disk and the repository.

### Conflict types

- **First seen** — a file exists at the target path but was never tracked by dotling
- **Both modified** — both the repo and local file changed since the last sync
- **Timestamp tie** — files differ but modification times are identical

### Snapshots

Snapshots used as the merge base are stored in `~/.dotling/snapshots/<source>` and are updated after each successful sync of a copy-mode plain file.

## Fingerprints

dotling uses Blake2s-256 content hashes stored in `~/.dotling/fingerprints.toml` for change detection. This avoids decrypting encrypted files just to check if they've changed.

- After each successful sync, dotling records the content hashes of the source, target, and (if encrypted) the `.enc` file
- On subsequent checks, dotling compares current hashes against stored fingerprints
- **Benefit:** `dotling status` and `dotling sync --dry-run` work instantly without entering your vault password

## Lifecycle Hooks

### Global hooks

Global hooks run at the very beginning and very end of the `dotling sync` session:

| Hook | When it runs |
|---|---|
| `init` | During `dotling init` (also runs when adopting or cloning) |
| `before` | Before any entries are synced |
| `after` | After all entries are successfully synced |

### Entry-level hooks

Each entry can define its own hooks:

| Hook | When it runs |
|---|---|
| `before` | Before this entry is pushed or pulled |
| `after` | After this entry is successfully pushed or pulled |

### Execution context

Hooks are executed in the repository root directory. The following environment variables are set:

| Variable | Description |
|---|---|
| `DOTLING_HOOK_TYPE` | `global_init`, `global_before`, `global_after`, `entry_before`, `entry_after` |
| `DOTLING_REPO_ROOT` | Absolute path to the dotfiles repository |
| `DOTLING_DRY_RUN` | `"true"` if running with `--dry-run`, otherwise `"false"` |
| `DOTLING_ENTRY_SOURCE` | (Entry hooks only) Repo-relative path of the entry's source |
| `DOTLING_ENTRY_TARGET` | (Entry hooks only) Target path of the entry's deployed file |
| `DOTLING_ENTRY_ACTION` | (Entry hooks only) Current action: `"push"` or `"pull"` |

### Hook trust system

To protect against malicious code in imported dotfile repositories, dotling prompts for verification before running a hook for the first time:

```text
  ⚡ Untrusted hook detected (type: entry_before):
    echo "updating shell configuration"
    ? Do you want to run this hook? [y]es (once) / [n]o (skip) / [a]lways (trust) / [s]kip all >
```

Selecting `always` stores the Blake2s-256 hash of the command string in `~/.dotling/state/trusted_hooks`.

- Pass `--allow-hooks` (or set `DOTLING_ALLOW_HOOKS=1`) to auto-execute all hooks
- Pass `--no-hooks` (or set `DOTLING_NO_HOOKS=1`) to disable all hooks

### Hook retry

If a hook exits with a non-zero status, dotling retries up to **3 times** (1 initial + 2 retries) before aborting.

## Flags

| Flag | Description |
|---|---|
| `--dry-run` | Show what would change without modifying anything |
| `--force` | Overwrite conflicting files without prompting (repo wins) |
| `--prefer-actual` | When both sides differ, prefer the local file (alias: `--prefer-local`) |
| `--no-interactive` | Skip conflicting entries and print a warning |
| `--allow-hooks` | Execute all hooks without prompting |
| `--no-hooks` | Disable all hook execution |
