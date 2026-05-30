//! Sync-state fingerprint store for encrypted entries.
//!
//! Since encrypted entries cannot be compared without decrypting, we track
//! content hashes of both the `.enc` file and the plaintext target after each
//! successful sync. On `status` we compare current hashes against the stored
//! ones to determine whether an entry is still in sync — no password required.
//!
//! # File format
//!
//! Stored at `~/.dotling/fingerprints.toml`:
//!
//! ```toml
//! # dotling sync fingerprints — managed by dotling
//!
//! [[entries]]
//! source      = "secrets/ssh_config"
//! enc_hash    = "a3f2..."
//! target_hash = "7c01..."
//! ```

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use blake2::{Blake2s256, Digest};

use crate::error::{Error, Result};

// ── Public types ──────────────────────────────────────────────────

/// Hashes recorded for a single encrypted entry at last-sync time.
#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub struct EntryFingerprint {
    /// Blake2s-256 hex digest of the `.enc` file bytes (encrypted entries).
    pub enc_hash: String,
    /// Blake2s-256 hex digest of the plaintext target bytes.
    pub target_hash: String,
    /// Blake2s-256 hex digest of the plaintext repo source file (copy-mode entries).
    /// Empty string for encrypted entries (we never hash plaintext of encrypted files).
    pub source_hash: String,
}

/// Which side(s) of a copy-mode entry have changed since the last sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhichSide {
    /// No fingerprint recorded yet — entry was never synced through dotling.
    Unknown,
    /// Neither side has changed since the last sync.
    Neither,
    /// Only the repo source file changed.
    RepoOnly,
    /// Only the actual (local) target file changed.
    ActualOnly,
    /// Both the repo source and the actual target changed.
    Both,
}

/// In-memory fingerprint store, backed by `~/.dotling/fingerprints.toml`.
pub struct FingerprintStore {
    records: HashMap<String, EntryFingerprint>,
    path: PathBuf,
    dirty: bool,
}

impl FingerprintStore {
    /// Load the store from disk. Returns an empty store if the file does not
    /// exist yet (i.e. no encrypted entries have been synced yet).
    pub fn load(path: PathBuf) -> Self {
        let records = if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .map(|s| parse_store(&s))
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self {
            records,
            path,
            dirty: false,
        }
    }

    /// Check whether a fingerprint record exists for `source`.
    pub fn has_record(&self, source: &str) -> bool {
        self.records.contains_key(source)
    }

    /// Compute and store hashes for `source` using the current on-disk
    /// contents of `enc_path` and `target_path` (encrypted entries).
    ///
    /// Call this after a **successful** push or pull of an encrypted entry.
    pub fn record(&mut self, source: &str, enc_path: &Path, target_path: &Path) -> Result<()> {
        let enc_hash = hash_path(enc_path)?;
        let target_hash = hash_path(target_path)?;
        self.records.insert(
            source.to_string(),
            EntryFingerprint {
                enc_hash,
                target_hash,
                source_hash: String::new(),
            },
        );
        self.dirty = true;
        Ok(())
    }

    /// Compute and store hashes for `source` using the current on-disk
    /// contents of `source_path` (the plain repo file) and `target_path`
    /// (the actual file on disk).  Use this for **non-encrypted copy-mode** entries.
    ///
    /// Also writes a plaintext snapshot of `target_path` to
    /// `~/.dotling/snapshots/<source>` so that future 3-way merges have a base.
    pub fn record_plain(
        &mut self,
        source: &str,
        source_path: &Path,
        target_path: &Path,
    ) -> Result<()> {
        let source_hash = hash_path(source_path)?;
        let target_hash = hash_path(target_path)?;
        self.records.insert(
            source.to_string(),
            EntryFingerprint {
                enc_hash: String::new(),
                target_hash,
                source_hash,
            },
        );
        self.dirty = true;
        Ok(())
    }

    /// Check whether `source` is still in sync with the on-disk files
    /// (encrypted entries).
    ///
    /// Returns:
    /// - `None`  — no fingerprint recorded yet.
    /// - `Some(true)`  — both hashes match (in sync).
    /// - `Some(false)` — at least one hash has changed.
    pub fn is_in_sync(&self, source: &str, enc_path: &Path, target_path: &Path) -> Option<bool> {
        let stored = self.records.get(source)?;

        let enc_ok = hash_path(enc_path).is_ok_and(|h| h == stored.enc_hash);
        let target_ok = hash_path(target_path).is_ok_and(|h| h == stored.target_hash);

        Some(enc_ok && target_ok)
    }

    /// Determine which side(s) of a **copy-mode** entry changed since the last
    /// sync by comparing current hashes against stored baselines.
    pub fn who_changed(&self, source: &str, source_path: &Path, target_path: &Path) -> WhichSide {
        let Some(stored) = self.records.get(source) else {
            return WhichSide::Unknown;
        };

        let source_same = hash_path(source_path).is_ok_and(|h| h == stored.source_hash);
        let target_same = hash_path(target_path).is_ok_and(|h| h == stored.target_hash);

        match (source_same, target_same) {
            (true, true) => WhichSide::Neither,
            (false, true) => WhichSide::RepoOnly,
            (true, false) => WhichSide::ActualOnly,
            (false, false) => WhichSide::Both,
        }
    }

    /// Persist the store to disk if any records were added or changed.
    pub fn save(&self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let content = serialize_store(&self.records);
        crate::fs::atomic_write(&self.path, content.as_bytes())
    }
}

// ── Hashing ───────────────────────────────────────────────────────

/// Compute a Blake2b-256 digest of a file's or directory's contents and return it as a
/// lowercase hex string.
pub fn hash_path(path: &Path) -> Result<String> {
    if path.is_dir() {
        let mut files = crate::fs::walk_dir(path, false)?;
        // Ensure sorted for deterministic hashing
        files.sort();

        let mut hasher = Blake2s256::new();
        for file in files {
            let rel_path = file.strip_prefix(path).unwrap_or(&file);
            hasher.update(rel_path.to_string_lossy().as_bytes());
            let content =
                fs::read(&file).map_err(|e| Error::io(&file, "read for fingerprint", e))?;
            hasher.update(&content);
        }
        let digest = hasher.finalize();
        Ok(hex_encode(&digest))
    } else {
        hash_file(path)
    }
}

/// Compute a Blake2b-256 digest of a file's contents and return it as a
/// lowercase hex string.
pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|e| Error::io(path, "read for fingerprint", e))?;
    let mut hasher = Blake2s256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(hex_encode(&digest))
}

fn hex_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        let _ = write!(out, "{b:02x}");
    }
    out
}

// ── Serialization ─────────────────────────────────────────────────

fn serialize_store(records: &HashMap<String, EntryFingerprint>) -> String {
    let mut out = String::from("# dotling sync fingerprints — managed by dotling\n\n");
    // Sort by source for stable output.
    let mut entries: Vec<_> = records.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());
    for (source, fp) in entries {
        let _ = writeln!(out, "[[entries]]");
        let _ = writeln!(out, "source      = \"{}\"", escape(source));
        if !fp.enc_hash.is_empty() {
            let _ = writeln!(out, "enc_hash    = \"{}\"", fp.enc_hash);
        }
        if !fp.source_hash.is_empty() {
            let _ = writeln!(out, "source_hash = \"{}\"", fp.source_hash);
        }
        let _ = writeln!(out, "target_hash = \"{}\"", fp.target_hash);
        let _ = writeln!(out);
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_store(input: &str) -> HashMap<String, EntryFingerprint> {
    let mut map = HashMap::new();
    let mut source: Option<String> = None;
    let mut enc_hash: Option<String> = None;
    let mut source_hash: Option<String> = None;
    let mut target_hash: Option<String> = None;

    for raw in input.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();

        if line == "[[entries]]" {
            // Flush previous record.
            if let (Some(s), Some(t)) = (source.take(), target_hash.take()) {
                map.insert(
                    s,
                    EntryFingerprint {
                        enc_hash: enc_hash.take().unwrap_or_default(),
                        source_hash: source_hash.take().unwrap_or_default(),
                        target_hash: t,
                    },
                );
            }
            enc_hash = None;
            source_hash = None;
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            match key {
                "source" => source = Some(val.to_string()),
                "enc_hash" => enc_hash = Some(val.to_string()),
                "source_hash" => source_hash = Some(val.to_string()),
                "target_hash" => target_hash = Some(val.to_string()),
                _ => {}
            }
        }
    }

    // Flush final record.
    if let (Some(s), Some(t)) = (source, target_hash) {
        map.insert(
            s,
            EntryFingerprint {
                enc_hash: enc_hash.unwrap_or_default(),
                source_hash: source_hash.unwrap_or_default(),
                target_hash: t,
            },
        );
    }

    map
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn hash_file_is_deterministic() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello dotling").unwrap();
        let h1 = hash_file(f.path()).unwrap();
        let h2 = hash_file(f.path()).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // Blake2b-256 → 32 bytes → 64 hex chars
    }

    #[test]
    fn hash_changes_with_content() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"version one").unwrap();
        let h1 = hash_file(f.path()).unwrap();
        f.as_file_mut().set_len(0).unwrap();
        f.write_all(b"version two").unwrap();
        let h2 = hash_file(f.path()).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn store_roundtrip() {
        let store_file = NamedTempFile::new().unwrap();
        let enc_file = NamedTempFile::new().unwrap();
        let tgt_file = NamedTempFile::new().unwrap();

        {
            let mut ef = enc_file.reopen().unwrap();
            ef.write_all(b"encrypted bytes").unwrap();
        }
        {
            let mut tf = tgt_file.reopen().unwrap();
            tf.write_all(b"plaintext bytes").unwrap();
        }

        let store_path = store_file.path().to_path_buf();

        {
            let mut store = FingerprintStore::load(store_path.clone());
            store
                .record("secrets/key", enc_file.path(), tgt_file.path())
                .unwrap();
            store.save().unwrap();
        }

        // Reload and check.
        let store2 = FingerprintStore::load(store_path);
        assert_eq!(
            store2.is_in_sync("secrets/key", enc_file.path(), tgt_file.path()),
            Some(true)
        );
        assert_eq!(
            store2.is_in_sync("secrets/other", enc_file.path(), tgt_file.path()),
            None
        );
    }

    #[test]
    fn detects_changed_target() {
        use std::io::Write;

        let store_file = NamedTempFile::new().unwrap();
        let enc_file = NamedTempFile::new().unwrap();
        let tgt_file = NamedTempFile::new().unwrap();

        enc_file.reopen().unwrap().write_all(b"enc").unwrap();
        tgt_file.reopen().unwrap().write_all(b"target v1").unwrap();

        let store_path = store_file.path().to_path_buf();
        let mut store = FingerprintStore::load(store_path.clone());
        store
            .record("a/b", enc_file.path(), tgt_file.path())
            .unwrap();
        store.save().unwrap();

        // Simulate user editing the target.
        std::fs::write(tgt_file.path(), b"target v2").unwrap();

        let store2 = FingerprintStore::load(store_path);
        assert_eq!(
            store2.is_in_sync("a/b", enc_file.path(), tgt_file.path()),
            Some(false)
        );
    }

    // ── who_changed tests ───────────────────────────────────────

    #[test]
    fn who_changed_unknown() {
        let store = FingerprintStore::load(NamedTempFile::new().unwrap().path().to_path_buf());
        let src = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        assert_eq!(
            store.who_changed("missing", src.path(), tgt.path()),
            WhichSide::Unknown
        );
    }

    #[test]
    fn who_changed_neither() {
        let store_file = NamedTempFile::new().unwrap();
        let src = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        std::fs::write(src.path(), "content").unwrap();
        std::fs::write(tgt.path(), "content").unwrap();

        let mut store = FingerprintStore::load(store_file.path().to_path_buf());
        store.record_plain("entry", src.path(), tgt.path()).unwrap();
        store.save().unwrap();

        let store2 = FingerprintStore::load(store_file.path().to_path_buf());
        assert_eq!(
            store2.who_changed("entry", src.path(), tgt.path()),
            WhichSide::Neither
        );
    }

    #[test]
    fn who_changed_repo_only() {
        let store_file = NamedTempFile::new().unwrap();
        let src = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        std::fs::write(src.path(), "original").unwrap();
        std::fs::write(tgt.path(), "original").unwrap();

        let mut store = FingerprintStore::load(store_file.path().to_path_buf());
        store.record_plain("entry", src.path(), tgt.path()).unwrap();
        store.save().unwrap();

        // Modify source only
        std::fs::write(src.path(), "modified source").unwrap();

        let store2 = FingerprintStore::load(store_file.path().to_path_buf());
        assert_eq!(
            store2.who_changed("entry", src.path(), tgt.path()),
            WhichSide::RepoOnly
        );
    }

    #[test]
    fn who_changed_actual_only() {
        let store_file = NamedTempFile::new().unwrap();
        let src = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        std::fs::write(src.path(), "original").unwrap();
        std::fs::write(tgt.path(), "original").unwrap();

        let mut store = FingerprintStore::load(store_file.path().to_path_buf());
        store.record_plain("entry", src.path(), tgt.path()).unwrap();
        store.save().unwrap();

        // Modify target only
        std::fs::write(tgt.path(), "modified target").unwrap();

        let store2 = FingerprintStore::load(store_file.path().to_path_buf());
        assert_eq!(
            store2.who_changed("entry", src.path(), tgt.path()),
            WhichSide::ActualOnly
        );
    }

    #[test]
    fn who_changed_both() {
        let store_file = NamedTempFile::new().unwrap();
        let src = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        std::fs::write(src.path(), "original").unwrap();
        std::fs::write(tgt.path(), "original").unwrap();

        let mut store = FingerprintStore::load(store_file.path().to_path_buf());
        store.record_plain("entry", src.path(), tgt.path()).unwrap();
        store.save().unwrap();

        // Modify both
        std::fs::write(src.path(), "new source").unwrap();
        std::fs::write(tgt.path(), "new target").unwrap();

        let store2 = FingerprintStore::load(store_file.path().to_path_buf());
        assert_eq!(
            store2.who_changed("entry", src.path(), tgt.path()),
            WhichSide::Both
        );
    }

    // ── record_plain tests ──────────────────────────────────────

    #[test]
    fn record_plain_roundtrip() {
        let store_file = NamedTempFile::new().unwrap();
        let src = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        std::fs::write(src.path(), "repo content").unwrap();
        std::fs::write(tgt.path(), "target content").unwrap();

        let mut store = FingerprintStore::load(store_file.path().to_path_buf());
        store
            .record_plain("copy/entry", src.path(), tgt.path())
            .unwrap();
        store.save().unwrap();

        let store2 = FingerprintStore::load(store_file.path().to_path_buf());
        assert_eq!(
            store2.who_changed("copy/entry", src.path(), tgt.path()),
            WhichSide::Neither
        );
    }

    // ── is_in_sync tests ────────────────────────────────────────

    #[test]
    fn is_in_sync_after_record() {
        let store_file = NamedTempFile::new().unwrap();
        let enc = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        std::fs::write(enc.path(), "encrypted").unwrap();
        std::fs::write(tgt.path(), "plaintext").unwrap();

        let mut store = FingerprintStore::load(store_file.path().to_path_buf());
        store.record("entry", enc.path(), tgt.path()).unwrap();

        assert_eq!(
            store.is_in_sync("entry", enc.path(), tgt.path()),
            Some(true)
        );
    }

    #[test]
    fn is_in_sync_unknown_source() {
        let store = FingerprintStore::load(NamedTempFile::new().unwrap().path().to_path_buf());
        let enc = NamedTempFile::new().unwrap();
        let tgt = NamedTempFile::new().unwrap();
        assert_eq!(store.is_in_sync("missing", enc.path(), tgt.path()), None);
    }

    // ── hash_path tests ─────────────────────────────────────────

    #[test]
    fn hash_path_directory() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("content");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "aaa").unwrap();
        fs::write(dir.join("b.txt"), "bbb").unwrap();

        let h = hash_path(&dir).unwrap();
        assert_eq!(h.len(), 64);
        // Deterministic
        assert_eq!(hash_path(&dir).unwrap(), h);
    }

    #[test]
    fn hash_path_nonexistent() {
        let temp = tempfile::tempdir().unwrap();
        let result = hash_path(&temp.path().join("nonexistent"));
        assert!(result.is_err());
    }

    // ── serialization tests ─────────────────────────────────────

    #[test]
    fn serialize_parse_roundtrip() {
        let mut records = HashMap::new();
        records.insert(
            "a/b".to_string(),
            EntryFingerprint {
                enc_hash: "abc123".to_string(),
                target_hash: "def456".to_string(),
                source_hash: String::new(),
            },
        );
        records.insert(
            "c/d".to_string(),
            EntryFingerprint {
                enc_hash: String::new(),
                target_hash: "789abc".to_string(),
                source_hash: "012def".to_string(),
            },
        );

        let serialized = serialize_store(&records);
        let parsed = parse_store(&serialized);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["a/b"].enc_hash, "abc123");
        assert_eq!(parsed["a/b"].target_hash, "def456");
        assert_eq!(parsed["c/d"].source_hash, "012def");
    }

    #[test]
    fn parse_empty_input() {
        let parsed = parse_store("");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_with_comments() {
        let input = "# comment\n[[entries]]\nsource = \"x\"\ntarget_hash = \"abc\"\n";
        let parsed = parse_store(input);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["x"].target_hash, "abc");
    }

    #[test]
    fn parse_with_missing_optional_fields() {
        let input = "[[entries]]\nsource = \"x\"\ntarget_hash = \"abc\"\n";
        let parsed = parse_store(input);
        assert_eq!(parsed["x"].enc_hash, "");
        assert_eq!(parsed["x"].source_hash, "");
    }
}
