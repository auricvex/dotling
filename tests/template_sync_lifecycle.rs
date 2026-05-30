//! Comprehensive integration tests for the template sync/status lifecycle.
//!
//! These tests verify that template entries correctly track sync state through
//! the full lifecycle: add → sync → status → encrypt → sync → status.
//!
//! Each test creates an isolated temp environment simulating the dotling
//! directory structure (repo + ~/.dotling/). Tests manipulate `$HOME` so
//! they must run serially.

#![allow(clippy::disallowed_types)]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use dotling::{
    commands::encrypt::encrypt_single_entry,
    config::{DeployMethod, Entry},
    deploy::{EntryState, check_state},
    fingerprint::{FingerprintStore, WhichSide},
    template::{RenderContext, render},
};
use serial_test::serial;

// ── Shared lock for HOME-sensitive tests ──────────────────────────

#[allow(clippy::disallowed_types)]
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ── Test environment ──────────────────────────────────────────────

struct TestEnv {
    home: PathBuf,
    repo: PathBuf,
    original_home: Option<String>,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnv {
    fn new() -> Self {
        let guard = lock_env();
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&repo).unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", &home) };

        // Register the repo in ~/.dotling/state.toml
        dotling::store::set_repo_root(&repo).unwrap();

        Self {
            home,
            repo,
            original_home,
            _guard: guard,
        }
    }

    fn dotling_dir(&self) -> PathBuf {
        self.home.join(".dotling")
    }

    fn fp_path(&self) -> PathBuf {
        self.dotling_dir().join("fingerprints.toml")
    }

    #[allow(clippy::unused_self)]
    fn make_entry(&self, source: &str, target: &str, template: bool) -> Entry {
        Entry {
            source: source.into(),
            target: target.into(),
            method: Some(DeployMethod::Copy),
            encrypted: false,
            directory: false,
            template,
            os: None,
            permissions: None,
            before: None,
            after: None,
        }
    }

    fn write_template(&self, rel_path: &str, content: &str) {
        let path = self.repo.join(rel_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }

    fn render_template(&self, entry: &Entry) -> String {
        let source_path = self.repo.join(&entry.source);
        let template_text = fs::read_to_string(&source_path).unwrap();
        let ctx = RenderContext {
            builtins: HashMap::from([
                ("hostname".into(), "testhost".into()),
                ("username".into(), "testuser".into()),
                ("os".into(), "macos".into()),
                ("arch".into(), "aarch64".into()),
                ("home".into(), self.home.to_string_lossy().into_owned()),
                ("repo".into(), self.repo.to_string_lossy().into_owned()),
            ]),
            vars: Vec::new(),
            env: HashMap::new(),
        };
        render(&template_text, &ctx, &entry.source).unwrap()
    }

    fn render_template_with_vars(&self, entry: &Entry, vars: Vec<(String, String)>) -> String {
        let source_path = self.repo.join(&entry.source);
        let template_text = fs::read_to_string(&source_path).unwrap();
        let ctx = RenderContext {
            builtins: HashMap::from([
                ("hostname".into(), "testhost".into()),
                ("username".into(), "testuser".into()),
                ("os".into(), "macos".into()),
                ("arch".into(), "aarch64".into()),
                ("home".into(), self.home.to_string_lossy().into_owned()),
                ("repo".into(), self.repo.to_string_lossy().into_owned()),
            ]),
            vars,
            env: HashMap::new(),
        };
        render(&template_text, &ctx, &entry.source).unwrap()
    }

    #[allow(clippy::unused_self)]
    fn deploy_rendered(&self, entry: &Entry, rendered: &str) {
        let target_path = dotling::path::expand_tilde(Path::new(&entry.target)).unwrap();
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        fs::write(&target_path, rendered).unwrap();
    }

    fn load_fp_store(&self) -> FingerprintStore {
        FingerprintStore::load(self.fp_path())
    }

    #[allow(clippy::unused_self)]
    fn save_fp_store(&self, store: &mut FingerprintStore) {
        store.save().unwrap();
    }

    fn deploy_state(&self, entry: &Entry) -> EntryState {
        check_state(entry, &self.repo, DeployMethod::Copy)
    }

    fn encrypt_source(&self, entry: &mut Entry) {
        let key = [0x42u8; 32];
        encrypt_single_entry(entry, &self.repo, &key).unwrap();
    }

    #[allow(clippy::unused_self)]
    fn expand_target(&self, entry: &Entry) -> PathBuf {
        dotling::path::expand_tilde(Path::new(&entry.target)).unwrap()
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(ref home) = self.original_home {
            unsafe { std::env::set_var("HOME", home) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────

fn source_path(env: &TestEnv, entry: &Entry) -> PathBuf {
    env.repo.join(&entry.source)
}

fn target_path(env: &TestEnv, entry: &Entry) -> PathBuf {
    env.expand_target(entry)
}

// ── Tests ─────────────────────────────────────────────────────────

#[test]
#[serial]
fn template_basic_lifecycle() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# zsh config\nexport PATH=$HOME/bin:$PATH");

    // Render + deploy (simulating first sync)
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // Verify deploy state is Deployed
    assert_eq!(env.deploy_state(&entry), EntryState::Deployed);

    // Record fingerprint (what sync does after rendering)
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Reload and verify status check: should be in sync
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );
}

#[test]
#[serial]
fn template_with_variables() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template(
        "shell/zshrc",
        "# config for {{ dotling.hostname }}\neditor={{ var.editor }}",
    );

    let vars = vec![("editor".into(), "vim".into())];
    let rendered = env.render_template_with_vars(&entry, vars);

    assert!(rendered.contains("testhost"), "hostname should resolve");
    assert!(rendered.contains("editor=vim"), "var.editor should resolve");

    env.deploy_rendered(&entry, &rendered);

    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );
}

#[test]
#[serial]
fn template_sync_no_change_is_idempotent() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# stable config");

    // First sync
    let rendered1 = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered1);
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Second sync: rendered output should match deployed file
    let rendered2 = env.render_template(&entry);
    assert_eq!(rendered1, rendered2, "rendering is deterministic");

    // Fingerprint still valid
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );
}

#[test]
#[serial]
fn template_source_changed_needs_sync() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# original");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Modify source in repo
    env.write_template("shell/zshrc", "# modified template");

    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::RepoOnly,
    );
}

#[test]
#[serial]
fn template_target_modified_needs_sync() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Modify deployed target file
    fs::write(target_path(&env, &entry), "# user modified").unwrap();

    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::ActualOnly,
    );
}

#[test]
#[serial]
fn template_both_changed_needs_sync() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# original");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Modify both sides
    env.write_template("shell/zshrc", "# repo modified");
    fs::write(target_path(&env, &entry), "# target modified").unwrap();

    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Both,
    );
}

#[test]
#[serial]
fn template_encrypt_then_sync_in_sync() {
    let env = TestEnv::new();
    let mut entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    // Template has no variables, so rendering is a passthrough
    env.write_template("shell/zshrc", "# my zsh config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // Record fingerprint (sync completed)
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Verify in sync before encrypt
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );

    // Encrypt the source file in-place
    env.encrypt_source(&mut entry);
    assert!(entry.encrypted);

    // After encrypt, source hash changed → who_changed should detect it
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::RepoOnly,
        "after encrypt, source hash should differ from stored fingerprint"
    );

    // Simulate sync: decrypt + render, output matches deployed file
    // (template has no variables, so rendered == original content)
    let key = [0x42u8; 32];
    let encrypted_bytes = fs::read(source_path(&env, &entry)).unwrap();
    let decrypted = dotling::crypto::decrypt_with_key(&encrypted_bytes, &key).unwrap();
    let decrypted_text = String::from_utf8(decrypted).unwrap();
    let ctx = RenderContext {
        builtins: HashMap::from([
            ("hostname".into(), "testhost".into()),
            ("username".into(), "testuser".into()),
            ("os".into(), "macos".into()),
            ("arch".into(), "aarch64".into()),
            ("home".into(), env.home.to_string_lossy().into_owned()),
            ("repo".into(), env.repo.to_string_lossy().into_owned()),
        ]),
        vars: Vec::new(),
        env: HashMap::new(),
    };
    let re_rendered = render(&decrypted_text, &ctx, &entry.source).unwrap();

    // Rendered output should match what's already deployed
    let current_target = fs::read_to_string(target_path(&env, &entry)).unwrap();
    assert_eq!(
        re_rendered, current_target,
        "re-rendered should match deployed"
    );

    // Sync updates fingerprint with new source hash (encrypted bytes)
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Now status should show in sync
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
        "after updating fingerprint, should be in sync"
    );
}

#[test]
#[serial]
fn template_encrypt_changes_source_hash() {
    let env = TestEnv::new();
    let mut entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Confirm in sync
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );

    // Encrypt source
    env.encrypt_source(&mut entry);

    // Stale fingerprint → RepoOnly (this was the bug)
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::RepoOnly,
        "encrypt changes source bytes, stale fingerprint should detect RepoOnly"
    );
}

#[test]
#[serial]
fn template_never_synced_needs_sync() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // No fingerprint recorded
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Unknown,
        "never-synced entry should return Unknown"
    );
}

#[test]
#[serial]
fn template_fingerprint_persists_across_reloads() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // Record and save
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Reload from disk
    let fp2 = env.load_fp_store();
    assert_eq!(
        fp2.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );

    // has_record should also work after reload
    assert!(fp2.has_record(&entry.source));
}

#[test]
#[serial]
fn encrypted_entry_fingerprint_tracking() {
    let env = TestEnv::new();
    let entry = env.make_entry("secrets/token", "~/.token", false);
    let enc_path = source_path(&env, &entry);
    let tgt_path = target_path(&env, &entry);

    // Create encrypted source and target
    let key = [0x42u8; 32];
    let encrypted = dotling::crypto::encrypt_with_key(b"secret data", &key).unwrap();
    fs::create_dir_all(enc_path.parent().unwrap()).unwrap();
    fs::write(&enc_path, &encrypted).unwrap();
    fs::create_dir_all(tgt_path.parent().unwrap()).unwrap();
    fs::write(&tgt_path, "secret data").unwrap();

    // Record via record() (encrypted path)
    let mut fp = env.load_fp_store();
    fp.record(&entry.source, &enc_path, &tgt_path).unwrap();
    env.save_fp_store(&mut fp);

    // Should be in sync
    let fp = env.load_fp_store();
    assert_eq!(
        fp.is_in_sync(&entry.source, &enc_path, &tgt_path),
        Some(true)
    );

    // Modify target
    fs::write(&tgt_path, "tampered data").unwrap();

    let fp = env.load_fp_store();
    assert_eq!(
        fp.is_in_sync(&entry.source, &enc_path, &tgt_path),
        Some(false)
    );
}

#[test]
#[serial]
fn encrypted_entry_unknown_source() {
    let env = TestEnv::new();
    let enc_path = env.repo.join("secrets/token");
    let tgt_path = env.home.join(".token");

    let fp = env.load_fp_store();
    assert_eq!(fp.is_in_sync("nonexistent", &enc_path, &tgt_path), None);
}

#[test]
#[serial]
fn deploy_state_template_always_deployed() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    // Source and target have completely different content
    env.write_template("shell/zshrc", "# template source");
    let target = target_path(&env, &entry);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "totally different content").unwrap();

    // Template entries always return Deployed (fingerprint-based comparison
    // happens separately in status/sync)
    assert_eq!(env.deploy_state(&entry), EntryState::Deployed);
}

#[test]
#[serial]
fn deploy_state_template_missing_target() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# template");
    // Target does NOT exist

    assert_eq!(env.deploy_state(&entry), EntryState::Missing);
}

#[test]
#[serial]
fn template_sync_backfills_missing_fingerprint() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // No fingerprint recorded (simulating add --template without sync)
    let fp = env.load_fp_store();
    assert!(!fp.has_record(&entry.source));
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Unknown,
    );

    // Simulate sync backfilling the fingerprint when rendered == deployed
    // (this is the fix we applied)
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Now status should show in sync
    let fp = env.load_fp_store();
    assert!(fp.has_record(&entry.source));
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );
}

#[test]
#[serial]
fn template_encrypt_decrypt_roundtrip_preserves_sync() {
    let env = TestEnv::new();
    let mut entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# my config\nalias ll='ls -la'");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // Sync: record fingerprint
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Encrypt
    env.encrypt_source(&mut entry);

    // Sync after encrypt: update fingerprint
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Verify in sync
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );

    // Decrypt
    let key = [0x42u8; 32];
    dotling::commands::encrypt::decrypt_single_entry(&mut entry, &env.repo, &key).unwrap();
    assert!(!entry.encrypted);

    // After decrypt, source hash changed again
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::RepoOnly,
        "decrypt changes source bytes back to plaintext"
    );

    // Sync again: update fingerprint
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );
}

#[test]
#[serial]
fn template_fingerprint_survives_file_persistence() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", true);

    env.write_template("shell/zshrc", "# persistent config");
    let rendered = env.render_template(&entry);
    env.deploy_rendered(&entry, &rendered);

    // Record, save, and verify the file exists on disk
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry.source,
        &source_path(&env, &entry),
        &target_path(&env, &entry),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    assert!(env.fp_path().exists(), "fingerprints.toml should exist");

    // Read the file and verify it contains our entry
    let content = fs::read_to_string(env.fp_path()).unwrap();
    assert!(
        content.contains("shell/zshrc"),
        "fingerprints file should contain our entry"
    );
    assert!(
        content.contains("source_hash"),
        "fingerprints file should have source_hash for plain entries"
    );
    assert!(
        content.contains("target_hash"),
        "fingerprints file should have target_hash"
    );

    // Reload and verify
    let fp2 = env.load_fp_store();
    assert!(fp2.has_record(&entry.source));
    assert_eq!(
        fp2.who_changed(
            &entry.source,
            &source_path(&env, &entry),
            &target_path(&env, &entry)
        ),
        WhichSide::Neither,
    );
}

#[test]
#[serial]
fn template_multiple_entries_independent() {
    let env = TestEnv::new();

    let entry_a = env.make_entry("shell/zshrc", "~/.zshrc", true);
    let entry_b = env.make_entry("shell/bashrc", "~/.bashrc", true);

    env.write_template("shell/zshrc", "# zsh config");
    env.write_template("shell/bashrc", "# bash config");

    // Deploy both
    let rendered_a = env.render_template(&entry_a);
    let rendered_b = env.render_template(&entry_b);
    env.deploy_rendered(&entry_a, &rendered_a);
    env.deploy_rendered(&entry_b, &rendered_b);

    // Record fingerprints for both A and B
    let mut fp = env.load_fp_store();
    fp.record_plain(
        &entry_a.source,
        &source_path(&env, &entry_a),
        &target_path(&env, &entry_a),
    )
    .unwrap();
    fp.record_plain(
        &entry_b.source,
        &source_path(&env, &entry_b),
        &target_path(&env, &entry_b),
    )
    .unwrap();
    env.save_fp_store(&mut fp);

    // Both should be in sync
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry_a.source,
            &source_path(&env, &entry_a),
            &target_path(&env, &entry_a)
        ),
        WhichSide::Neither,
    );
    assert_eq!(
        fp.who_changed(
            &entry_b.source,
            &source_path(&env, &entry_b),
            &target_path(&env, &entry_b)
        ),
        WhichSide::Neither,
    );

    // Modify B's source
    env.write_template("shell/bashrc", "# modified bash config");

    // A still in sync, B is RepoOnly
    let fp = env.load_fp_store();
    assert_eq!(
        fp.who_changed(
            &entry_a.source,
            &source_path(&env, &entry_a),
            &target_path(&env, &entry_a)
        ),
        WhichSide::Neither,
    );
    assert_eq!(
        fp.who_changed(
            &entry_b.source,
            &source_path(&env, &entry_b),
            &target_path(&env, &entry_b)
        ),
        WhichSide::RepoOnly,
    );
}

#[test]
#[serial]
fn has_record_false_for_missing() {
    let env = TestEnv::new();
    let fp = env.load_fp_store();
    assert!(!fp.has_record("nonexistent/path"));
}

#[test]
#[serial]
fn deploy_state_plain_copy_modified() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", false);

    let source = source_path(&env, &entry);
    let target = target_path(&env, &entry);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&source, "same content").unwrap();
    fs::write(&target, "different content").unwrap();

    // Plain copy entry with different content → Modified
    assert_eq!(env.deploy_state(&entry), EntryState::Modified);
}

#[test]
#[serial]
fn deploy_state_plain_copy_deployed() {
    let env = TestEnv::new();
    let entry = env.make_entry("shell/zshrc", "~/.zshrc", false);

    let source = source_path(&env, &entry);
    let target = target_path(&env, &entry);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&source, "same content").unwrap();
    fs::write(&target, "same content").unwrap();

    // Plain copy entry with identical content → Deployed
    assert_eq!(env.deploy_state(&entry), EntryState::Deployed);
}
