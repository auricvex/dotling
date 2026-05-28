use std::{fs, path::PathBuf};

use crate::{
    error::{Error, Result},
    store,
};

// ── Data model ─────────────────────────────────────────────────────

/// Machine-local variable store backed by `~/.dotling/vars.toml`.
///
/// Variables in this store take priority over shared defaults in `dotling.toml`.
/// This file is never committed to git.
#[derive(Debug, Clone, Default)]
pub struct VarStore {
    /// Ordered list of `(key, value)` pairs preserving insertion order.
    vars: Vec<(String, String)>,
}

impl VarStore {
    /// Load the local var store from `~/.dotling/vars.toml`.
    ///
    /// If the file does not exist, returns an empty store (not an error —
    /// this is normal on a fresh machine).
    pub fn load() -> Result<Self> {
        let path = store::vars_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| Error::io(&path, "read vars", e))?;
        Ok(Self::parse(&content))
    }

    /// Save the var store to `~/.dotling/vars.toml`.
    pub fn save(&self) -> Result<()> {
        let path = store::vars_path()?;

        // Ensure the state directory exists.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::io(parent, "create state directory", e))?;
        }

        let content = self.serialize();
        crate::fs::atomic_write(&path, content.as_bytes())
    }

    /// Get the value of a variable by key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Set a variable. Overwrites an existing key or appends if new.
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(entry) = self.vars.iter_mut().find(|(k, _)| k == key) {
            entry.1 = value.to_string();
        } else {
            self.vars.push((key.to_string(), value.to_string()));
        }
    }

    /// Remove a variable. Returns `true` if it was present.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.vars.len();
        self.vars.retain(|(k, _)| k != key);
        self.vars.len() < before
    }

    /// Iterate over all `(key, value)` pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Return a slice of all vars as `(String, String)` pairs
    /// (compatible with `RenderContext::new`).
    pub fn as_pairs(&self) -> Vec<(String, String)> {
        self.vars.clone()
    }

    /// Return true if the store contains no variables.
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// Number of variables in the store.
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    /// Path where the store is persisted.
    pub fn path() -> Result<PathBuf> {
        store::vars_path()
    }
}

// ── Parsing ────────────────────────────────────────────────────────

impl VarStore {
    fn parse(content: &str) -> Self {
        let mut vars = Vec::new();
        let mut in_vars_section = false;

        for raw_line in content.lines() {
            // Strip inline comments and trim
            let line = raw_line.split('#').next().unwrap_or("").trim();

            if line.is_empty() {
                continue;
            }

            // Section header
            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim();
                in_vars_section = section == "vars";
                continue;
            }

            if !in_vars_section {
                continue;
            }

            // Key = value
            if let Some((key, rest)) = line.split_once('=') {
                let key = key.trim().to_string();
                let raw_val = rest.trim();
                let value = if (raw_val.starts_with('"') && raw_val.ends_with('"'))
                    || (raw_val.starts_with('\'') && raw_val.ends_with('\''))
                {
                    unescape_str(&raw_val[1..raw_val.len() - 1])
                } else {
                    raw_val.to_string()
                };
                if !key.is_empty() {
                    vars.push((key, value));
                }
            }
        }

        Self { vars }
    }

    fn serialize(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# ~/.dotling/vars.toml — machine-local variables, NOT committed to git"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "[vars]");
        for (key, value) in &self.vars {
            let escaped = escape_str(value);
            let _ = writeln!(out, "{key} = \"{escaped}\"");
        }
        out
    }
}

fn unescape_str(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some(ch @ ('\\' | '"' | '\'')) => result.push(ch),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

// ── Bulk import ────────────────────────────────────────────────────

/// Import variables from a TOML file (any `[vars]` section) or a `.env` file.
///
/// Returns the number of variables imported.
pub fn import_from_file(store: &mut VarStore, path: &std::path::Path) -> Result<usize> {
    let content = fs::read_to_string(path).map_err(|e| Error::io(path, "read import file", e))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let pairs: Vec<(String, String)> =
        if ext == "env" || path.file_name().and_then(|n| n.to_str()) == Some(".env") {
            parse_env_file(&content)
        } else {
            // Treat as TOML-like with a [vars] section
            VarStore::parse(&content).vars
        };

    let count = pairs.len();
    for (k, v) in pairs {
        store.set(&k, &v);
    }
    Ok(count)
}

fn parse_env_file(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim().to_string();
            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
            if !key.is_empty() {
                pairs.push((key, val));
            }
        }
    }
    pairs
}

// ── Doctor helpers ─────────────────────────────────────────────────

/// Heuristically check if a committed [vars] default value looks like a real value
/// that should be in `vars.toml` instead.
pub fn looks_like_real_value(key: &str, value: &str, local_store: &VarStore) -> Option<String> {
    // Identical to a local vars.toml value (copy-paste mistake)
    if let Some(local_val) = local_store.get(key) {
        if local_val == value {
            return Some(format!(
                "`{key} = \"{value}\"` matches your local vars.toml — use a placeholder instead"
            ));
        }
    }

    // Looks like an email address
    if value.contains('@') && value.contains('.') {
        return Some(format!(
            "`{key}` value looks like an email address — move to vars.toml"
        ));
    }

    // Suspiciously long (possible token/key)
    if value.len() > 40 {
        return Some(format!(
            "`{key}` value is very long ({} chars) — may be a secret, move to vars.toml",
            value.len()
        ));
    }

    // Matches current system username
    if let Ok(username) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        if !username.is_empty() && value == username {
            return Some(format!(
                "`{key} = \"{value}\"` matches current username — use a placeholder like \"user\""
            ));
        }
    }

    None
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn vars_roundtrip() {
        let mut store = VarStore::default();
        store.set("foo", "bar");
        store.set("baz", "qux");

        let serialized = store.serialize();
        let loaded = VarStore::parse(&serialized);

        assert_eq!(loaded.get("foo"), Some("bar"));
        assert_eq!(loaded.get("baz"), Some("qux"));
    }

    #[test]
    fn vars_missing_file_ok() {
        // An empty VarStore is returned for a missing file
        let store = VarStore::default();
        assert!(store.is_empty());
    }

    #[test]
    fn vars_set_overwrites() {
        let mut store = VarStore::default();
        store.set("key", "old");
        store.set("key", "new");
        assert_eq!(store.get("key"), Some("new"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn vars_remove() {
        let mut store = VarStore::default();
        store.set("a", "1");
        store.set("b", "2");
        assert!(store.remove("a"));
        assert!(!store.remove("a")); // already gone
        assert_eq!(store.get("a"), None);
        assert_eq!(store.get("b"), Some("2"));
    }

    #[test]
    fn vars_iter_order() {
        let mut store = VarStore::default();
        store.set("c", "3");
        store.set("a", "1");
        store.set("b", "2");
        let keys: Vec<&str> = store.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec!["c", "a", "b"]);
    }

    #[test]
    fn parse_env_file_basic() {
        let content = "FOO=bar\nBAZ=\"qux\"\n# comment\n\nEMPTY=";
        let pairs = parse_env_file(content);
        let map: HashMap<_, _> = pairs.into_iter().collect();
        assert_eq!(map.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(map.get("BAZ"), Some(&"qux".to_string()));
        assert_eq!(map.get("EMPTY"), Some(&String::new()));
        assert!(!map.contains_key("comment"));
    }

    #[test]
    fn parse_vars_toml_ignores_other_sections() {
        let content = "[other]\nfoo = \"should_ignore\"\n[vars]\nbar = \"keep\"";
        let store = VarStore::parse(content);
        assert_eq!(store.get("bar"), Some("keep"));
        assert_eq!(store.get("foo"), None);
    }
}
