use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    config::Config,
    error::{Error, Result},
    store, ui,
};

// ── Public entry point ─────────────────────────────────────────────

/// Edit a tracked entry in the user's `$EDITOR`.
///
/// Behaviour by entry type:
/// - **Encrypted**: decrypt → temp file → editor → re-encrypt → write `.enc`.
/// - **Plain template / copy / symlink**: open repo source directly.
pub fn run(query: &str) -> Result<()> {
    let repo_root = store::require_repo_root()?;
    let config_path = store::config_path(&repo_root);
    let config = Config::load(&config_path)?;

    let entry = config.find_entry(query).ok_or_else(|| {
        Error::User(format!(
            "`{query}` is not tracked — use `dotling status` to list tracked entries"
        ))
    })?;

    // Clone what we need before the borrow ends.
    let entry = entry.clone();

    let editor = find_editor()?;

    if entry.encrypted {
        run_encrypted_edit(&entry, &repo_root, &editor)
    } else {
        run_plain_edit(&entry, &repo_root, &editor)
    }
}

// ── Encrypted edit ────────────────────────────────────────────────

fn run_encrypted_edit(entry: &crate::config::Entry, repo_root: &Path, editor: &str) -> Result<()> {
    let enc_path = repo_root.join(&entry.source);

    if !enc_path.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!(
                "encrypted source `{}` not found in repo",
                enc_path.display()
            ),
        });
    }

    // Prompt for vault password and derive master key.
    let password = ui::password("Vault password");
    let master_key = crate::crypto::vault::unlock_vault(&password)?;

    if entry.directory {
        edit_encrypted_directory(entry, &enc_path, &master_key, editor, repo_root)
    } else {
        edit_encrypted_file(entry, &enc_path, &master_key, editor, repo_root)
    }
}

/// Edit a single encrypted file: decrypt → temp → editor → re-encrypt.
fn edit_encrypted_file(
    entry: &crate::config::Entry,
    enc_path: &Path,
    master_key: &[u8; 32],
    editor: &str,
    repo_root: &Path,
) -> Result<()> {
    // Decrypt.
    let ciphertext =
        fs::read(enc_path).map_err(|e| Error::io(enc_path, "read encrypted file", e))?;
    let plaintext = crate::crypto::decrypt_with_key(&ciphertext, master_key)?;

    // Derive a safe temp file name from the original source basename.
    let basename = Path::new(&entry.source).file_name().map_or_else(
        || "dotling-edit".to_string(),
        |n| {
            // Strip .enc suffix from the basename for a nicer editor title.
            // source is like "shell/zshrc", basename "zshrc" — no .enc here
            // but encrypted templates have "shell/zshrc.dtmpl", basename "zshrc.dtmpl"
            n.to_string_lossy().into_owned()
        },
    );

    let tmp_dir = make_secure_temp_dir(repo_root)?;
    let tmp_path = tmp_dir.join(&basename);

    // Write plaintext to temp file with secure permissions.
    write_secure(&tmp_path, &plaintext)?;

    // Hash before edit.
    let hash_before = blake2_hash(&plaintext);

    // Launch editor — this blocks until the user closes it.
    let changed = launch_editor_and_check(editor, &tmp_path, &hash_before)?;

    if !changed {
        ui::info("no changes — skipping re-encryption");
        secure_remove(&tmp_path);
        remove_temp_dir(&tmp_dir);
        return Ok(());
    }

    // Read back the edited content.
    let edited =
        fs::read(&tmp_path).map_err(|e| Error::io(&tmp_path, "read edited temp file", e))?;

    // Re-encrypt and write back to repo atomically.
    let new_ciphertext = crate::crypto::encrypt_with_key(&edited, master_key)?;
    crate::fs::atomic_write(enc_path, &new_ciphertext)?;

    // Wipe the temp file.
    secure_remove(&tmp_path);
    remove_temp_dir(&tmp_dir);

    // Update fingerprint so status/sync stays accurate.
    update_fingerprint_encrypted(entry, enc_path, repo_root);

    ui::success(&format!("saved and re-encrypted `{}`", entry.source));

    // Remind the user to sync so the deployed file is updated.
    ui::hint("run `dotling sync` to push the changes to your deployed file");

    Ok(())
}

/// For an encrypted directory: edit each .enc file in sequence.
fn edit_encrypted_directory(
    entry: &crate::config::Entry,
    dir_path: &Path,
    master_key: &[u8; 32],
    editor: &str,
    repo_root: &Path,
) -> Result<()> {
    let enc_files = collect_enc_files(dir_path)?;

    if enc_files.is_empty() {
        ui::info("no encrypted files found in directory");
        return Ok(());
    }

    ui::header(&format!(
        "Editing {} encrypted file{} in `{}`",
        enc_files.len(),
        if enc_files.len() == 1 { "" } else { "s" },
        entry.source
    ));

    let tmp_dir = make_secure_temp_dir(repo_root)?;
    let mut edited_count = 0usize;

    for enc_file in &enc_files {
        // Build a display name relative to the dir.
        let rel = enc_file
            .strip_prefix(dir_path)
            .unwrap_or(enc_file)
            .display()
            .to_string();

        ui::info(&format!("editing `{rel}`"));

        let ciphertext =
            fs::read(enc_file).map_err(|e| Error::io(enc_file, "read encrypted file", e))?;
        let plaintext = crate::crypto::decrypt_with_key(&ciphertext, master_key)?;

        // Strip .enc from the temp file name so the editor uses the right syntax.
        let tmp_name = enc_file.file_stem().map_or_else(
            || "dotling-edit".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let tmp_path = tmp_dir.join(&tmp_name);

        write_secure(&tmp_path, &plaintext)?;
        let hash_before = blake2_hash(&plaintext);

        let changed = launch_editor_and_check(editor, &tmp_path, &hash_before)?;

        if changed {
            let edited =
                fs::read(&tmp_path).map_err(|e| Error::io(&tmp_path, "read edited file", e))?;
            let new_ciphertext = crate::crypto::encrypt_with_key(&edited, master_key)?;
            crate::fs::atomic_write(enc_file, &new_ciphertext)?;
            edited_count += 1;
            ui::success(&format!("saved `{rel}`"));
        } else {
            ui::dim(&format!("  no changes — skipped `{rel}`"));
        }

        secure_remove(&tmp_path);
    }

    remove_temp_dir(&tmp_dir);

    if edited_count > 0 {
        update_fingerprint_encrypted(entry, dir_path, repo_root);
        ui::hint("run `dotling sync` to push the changes to your deployed files");
    }

    ui::summary(edited_count, 0, 0);
    Ok(())
}

// ── Plain edit ────────────────────────────────────────────────────

fn run_plain_edit(entry: &crate::config::Entry, repo_root: &Path, editor: &str) -> Result<()> {
    let source_path = repo_root.join(&entry.source);

    if !source_path.exists() {
        return Err(Error::Deploy {
            entry: entry.source.clone(),
            message: format!("source `{}` not found in repo", source_path.display()),
        });
    }

    if entry.template {
        ui::info(&format!(
            "editing template source `{}` (use `dotling sync` to redeploy after saving)",
            entry.source
        ));
    } else if entry.directory {
        // For directories: list files and let user pick, or edit all — for now
        // open the directory path and let the editor handle it (works with neovim, etc.)
        ui::info(&format!("opening directory `{}` in editor", entry.source));
    }

    // Launch editor directly on the repo file.
    launch_editor(editor, &source_path)?;

    if !entry.template {
        ui::hint("run `dotling sync` to push the changes to your deployed file");
    }

    Ok(())
}

// ── Editor detection & launch ─────────────────────────────────────

/// Find a usable editor from the environment or well-known fallbacks.
///
/// Priority: `$DOTLING_EDITOR` → `$VISUAL` → `$EDITOR` → `vim` → `nano`
///
/// GUI editors that normally detach immediately (VS Code, Sublime Text, Zed,
/// Pulsar) are automatically given their "wait" flag so dotling blocks until
/// the user closes the file.
fn find_editor() -> Result<String> {
    // Environment variables, in priority order.
    for var in &["DOTLING_EDITOR", "VISUAL", "EDITOR"] {
        if let Ok(val) = std::env::var(var) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                return Ok(normalize_gui_editor(val));
            }
        }
    }

    // Well-known fallback editors.
    for candidate in &["vim", "nano", "vi"] {
        if which(candidate) {
            return Ok((*candidate).to_string());
        }
    }

    Err(Error::User(
        "no editor found — set $EDITOR, $VISUAL, or $DOTLING_EDITOR".into(),
    ))
}

/// Ensure GUI editors that fork immediately are given their "wait" flag.
///
/// | Binary   | Required flag    |
/// |----------|------------------|
/// | `code`   | `--wait`         |
/// | `subl`   | `--wait`         |
/// | `zed`    | `--wait`         |
/// | `pulsar` | `--wait`         |
/// | `atom`   | `--wait`         |
///
/// If the flag is already present (e.g. `VISUAL=code --wait`) this is a no-op.
fn normalize_gui_editor(editor: String) -> String {
    // Binaries that require --wait to block.
    const NEEDS_WAIT: &[&str] = &["code", "subl", "zed", "pulsar", "atom"];

    let bin = editor
        .split_whitespace()
        .next()
        .unwrap_or(&editor)
        .to_string();

    // Match just the filename component (handles full paths like /usr/bin/code).
    let bin_name = std::path::Path::new(&bin)
        .file_name()
        .map_or_else(|| bin.clone(), |n| n.to_string_lossy().into_owned());

    let is_gui = NEEDS_WAIT
        .iter()
        .any(|&name| bin_name.eq_ignore_ascii_case(name));

    if is_gui && !editor.contains("--wait") {
        // Append --wait so dotling blocks until the tab is closed.
        format!("{editor} --wait")
    } else {
        editor
    }
}

/// Check whether `cmd` is reachable on `$PATH`.
fn which(cmd: &str) -> bool {
    // Try `which <cmd>` or `command -v <cmd>` on Unix.
    #[cfg(unix)]
    {
        Command::new("sh")
            .args(["-c", &format!("command -v {cmd}")])
            .output()
            .is_ok_and(|o| o.status.success())
    }
    #[cfg(not(unix))]
    {
        Command::new("where")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Launch the editor on `path`, blocking until the process exits.
fn launch_editor(editor: &str, path: &Path) -> Result<()> {
    // Split editor string so "code --wait" works correctly.
    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (bin, args) = parts.split_first().unwrap_or((&"vi", &[]));

    let status = Command::new(bin)
        .args(args)
        .arg(path)
        .status()
        .map_err(|e| {
            Error::User(format!(
                "could not launch editor `{editor}`: {e}\n    \
                 Tip: set $EDITOR or $DOTLING_EDITOR to your preferred editor"
            ))
        })?;

    if !status.success() {
        // Many editors (e.g. vim) exit non-zero on :cq or signals.
        // We treat any exit as "done" rather than an error; dirty detection
        // handles whether content actually changed.
        ui::warning(&format!(
            "editor exited with status {status} — check your changes"
        ));
    }

    Ok(())
}

/// Launch editor and return `true` if the file content changed.
fn launch_editor_and_check(editor: &str, path: &Path, hash_before: &[u8]) -> Result<bool> {
    launch_editor(editor, path)?;

    // Re-read and hash after editing.
    let after = fs::read(path).map_err(|e| Error::io(path, "read file after edit", e))?;
    let hash_after = blake2_hash(&after);

    Ok(hash_after != hash_before)
}

// ── Secure temp file helpers ──────────────────────────────────────

/// Create a mode-700 temp directory under `~/.dotling/tmp/`.
fn make_secure_temp_dir(repo_root: &Path) -> Result<PathBuf> {
    // Use the dotling state dir so it stays within the user's home,
    // never in /tmp which could be world-readable on some systems.
    let base = store::state_dir().unwrap_or_else(|_| {
        // Fallback: a tmp subdir next to the repo.
        repo_root.join(".dotling-tmp")
    });

    // Use a unique-enough subdir: PID + a counter.
    let unique = format!("edit-{}-{}", std::process::id(), timestamp_nanos());
    let dir = base.join("tmp").join(unique);

    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, "create temp directory", e))?;

    // Set permissions to 700 (owner-only) on Unix.
    #[cfg(unix)]
    crate::fs::set_permissions(&dir, 0o700)?;

    Ok(dir)
}

/// Write `data` to `path` with 0o600 permissions.
fn write_secure(path: &Path, data: &[u8]) -> Result<()> {
    crate::fs::atomic_write(path, data)?;
    #[cfg(unix)]
    crate::fs::set_permissions(path, 0o600)?;
    Ok(())
}

/// Best-effort overwrite-with-zeros then remove of a file.
fn secure_remove(path: &Path) {
    if !path.exists() {
        return;
    }
    // Overwrite with zeros before unlinking (best-effort, not cryptographically
    // guaranteed on all filesystems, but much better than a plain remove).
    if let Ok(Ok(len)) = path.metadata().map(|m| usize::try_from(m.len())) {
        let zeros = vec![0u8; len];
        let _ = crate::fs::atomic_write(path, &zeros);
    }
    let _ = fs::remove_file(path);
}

/// Remove the temp directory (best-effort, silently ignore errors).
fn remove_temp_dir(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

// ── Directory helpers ─────────────────────────────────────────────

/// Recursively collect all `.enc` files under `dir`.
fn collect_enc_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_enc_files_inner(dir, &mut files)?;
    files.sort(); // deterministic order
    Ok(files)
}

fn collect_enc_files_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, "read directory", e))? {
        let entry = entry.map_err(|e| Error::io(dir, "read directory entry", e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_enc_files_inner(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("enc") {
            out.push(path);
        }
    }
    Ok(())
}

// ── Fingerprint update ────────────────────────────────────────────

/// Update the fingerprint store after re-encrypting an entry so that
/// `dotling status` / `dotling sync --dry-run` stay accurate.
fn update_fingerprint_encrypted(entry: &crate::config::Entry, enc_path: &Path, _repo_root: &Path) {
    let Ok(fp_path) = store::fingerprint_path() else {
        return;
    };
    let mut fp_store = crate::fingerprint::FingerprintStore::load(fp_path);

    if let Ok(target_path) = crate::path::expand_tilde(std::path::Path::new(&entry.target)) {
        if fp_store
            .record(&entry.source, enc_path, &target_path)
            .is_ok()
        {
            let _ = fp_store.save();
        }
    }
}

// ── Misc helpers ──────────────────────────────────────────────────

/// Simple Blake2b-512 hash of `data`.
fn blake2_hash(data: &[u8]) -> Vec<u8> {
    use blake2::{Blake2b512, Digest};
    let mut h = Blake2b512::new();
    h.update(data);
    h.finalize().to_vec()
}

/// Nanoseconds since UNIX epoch (for unique temp dir names).
fn timestamp_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A testable version of `find_editor` that reads from an explicit map
    /// instead of the real process environment, avoiding parallel test races.
    fn find_editor_from_map(env: &[(&str, &str)]) -> Option<String> {
        for var in &["DOTLING_EDITOR", "VISUAL", "EDITOR"] {
            if let Some(val) = env.iter().find(|(k, _)| k == var).map(|(_, v)| *v) {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
        None
    }

    #[test]
    fn find_editor_from_env() {
        // DOTLING_EDITOR must beat VISUAL and EDITOR.
        let env = [
            ("DOTLING_EDITOR", "emacs"),
            ("VISUAL", "code --wait"),
            ("EDITOR", "nano"),
        ];
        assert_eq!(find_editor_from_map(&env).as_deref(), Some("emacs"));
    }

    #[test]
    fn find_editor_falls_back_to_visual() {
        // No DOTLING_EDITOR → VISUAL wins.
        let env = [("VISUAL", "code --wait"), ("EDITOR", "nano")];
        assert_eq!(find_editor_from_map(&env).as_deref(), Some("code --wait"));
    }

    #[test]
    fn find_editor_falls_back_to_editor() {
        // No DOTLING_EDITOR, no VISUAL → EDITOR wins.
        let env = [("EDITOR", "nano")];
        assert_eq!(find_editor_from_map(&env).as_deref(), Some("nano"));
    }

    #[test]
    fn find_editor_returns_none_when_all_empty() {
        assert_eq!(find_editor_from_map(&[]).as_deref(), None);
    }

    #[test]
    fn normalize_adds_wait_for_vscode() {
        assert_eq!(normalize_gui_editor("code".into()), "code --wait");
    }

    #[test]
    fn normalize_adds_wait_for_vscode_full_path() {
        assert_eq!(
            normalize_gui_editor("/usr/local/bin/code".into()),
            "/usr/local/bin/code --wait"
        );
    }

    #[test]
    fn normalize_does_not_duplicate_wait() {
        // Already has --wait — must not add it again.
        assert_eq!(normalize_gui_editor("code --wait".into()), "code --wait");
    }

    #[test]
    fn normalize_adds_wait_for_subl() {
        assert_eq!(normalize_gui_editor("subl".into()), "subl --wait");
    }

    #[test]
    fn normalize_does_not_touch_terminal_editors() {
        // Terminal editors should not get --wait injected.
        assert_eq!(normalize_gui_editor("vim".into()), "vim");
        assert_eq!(normalize_gui_editor("nano".into()), "nano");
        assert_eq!(normalize_gui_editor("nvim".into()), "nvim");
        assert_eq!(normalize_gui_editor("emacs".into()), "emacs");
    }

    #[test]
    fn blake2_hash_deterministic() {
        let a = blake2_hash(b"hello");
        let b = blake2_hash(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn blake2_hash_differs_on_different_input() {
        let a = blake2_hash(b"hello");
        let b = blake2_hash(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn blake2_hash_detects_change() {
        let original = b"original content";
        let modified = b"modified content";
        let h1 = blake2_hash(original);
        let h2 = blake2_hash(modified);
        assert_ne!(h1, h2, "hashes must differ after edit");
    }
}
