/// Configuration file (`.dotling.toml`) management.
///
/// Each tracked file is represented by a [`LinkEntry`] with a repo-relative
/// source path, an absolute destination path, and a [`LinkMethod`].
/// The config lives at the root of the dotling repository.
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{DotlingError, Result, io_err},
    platform::Platform,
};

/// Name of the config file at the repo root.
pub const CONFIG_FILE: &str = ".dotling.toml";

/// How a file should be deployed to its destination.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkMethod {
    /// Deploy as a symbolic link (default).
    #[default]
    Symlink,
    /// Deploy as a file copy.
    Copy,
    /// Deploy as an age-encrypted copy.
    Encrypted,
}

impl std::fmt::Display for LinkMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symlink => write!(f, "symlink"),
            Self::Copy => write!(f, "copy"),
            Self::Encrypted => write!(f, "encrypted"),
        }
    }
}

/// A single tracked file entry in the config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkEntry {
    /// Repo-relative source path (forward slashes only).
    pub src: String,
    /// Absolute destination path (`~` prefix allowed).
    pub dest: String,
    /// How the file is deployed.
    #[serde(default)]
    pub method: LinkMethod,
    /// Target OS for this entry.
    #[serde(default)]
    pub os: Platform,
}

/// Global encryption settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// List of age public key strings (recipients).
    #[serde(default)]
    pub recipients: Vec<String>,
}

/// Wrapper for serialization with `[[links]]` table syntax.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConfigFile {
    /// Global encryption settings.
    #[serde(default)]
    encryption: EncryptionConfig,
    /// All tracked link entries.
    #[serde(default)]
    links: Vec<LinkEntry>,
}

/// The dotling configuration, managing tracked file entries.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the config file on disk.
    path: PathBuf,
    /// Global encryption settings.
    pub encryption: EncryptionConfig,
    /// All tracked entries, in insertion order.
    pub entries: Vec<LinkEntry>,
}

impl Config {
    /// Loads the config from the given repo root directory.
    ///
    /// Returns an empty config if the file does not exist yet.
    pub fn load(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Self {
                path,
                encryption: EncryptionConfig::default(),
                entries: Vec::new(),
            });
        }
        let content = fs::read_to_string(&path).map_err(io_err(&path))?;
        let file: ConfigFile =
            toml::from_str(&content).map_err(|e| DotlingError::ConfigParse(e.to_string()))?;
        Ok(Self {
            path,
            encryption: file.encryption,
            entries: file.links,
        })
    }

    /// Saves the config to disk with human-readable TOML formatting.
    ///
    /// Produces `[[links]]` array-of-tables syntax with blank lines between
    /// entries for readability.
    pub fn save(&self) -> Result<()> {
        let mut output = String::new();

        if !self.encryption.recipients.is_empty() {
            output.push_str("[encryption]\n");
            output.push_str("recipients = [\n");
            for recipient in &self.encryption.recipients {
                let _ = writeln!(output, "    {recipient:?},");
            }
            output.push_str("]\n\n");
        }

        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str("[[links]]\n");
            let _ = writeln!(output, "src = {:?}", entry.src);
            let _ = writeln!(output, "dest = {:?}", entry.dest);
            if entry.method != LinkMethod::Symlink {
                let _ = writeln!(output, "method = \"{}\"", entry.method);
            }
            if entry.os != Platform::All {
                let _ = writeln!(output, "os = \"{}\"", entry.os);
            }
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        fs::write(&self.path, &output).map_err(io_err(&self.path))?;
        Ok(())
    }

    /// Adds a new entry. Errors if the destination is already tracked.
    pub fn add_entry(&mut self, entry: LinkEntry) -> Result<()> {
        if self.entries.iter().any(|e| e.dest == entry.dest) {
            return Err(DotlingError::AlreadyTracked(PathBuf::from(&entry.dest)));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Removes an entry by its destination path. Errors if not found.
    pub fn remove_entry(&mut self, dest: &str) -> Result<LinkEntry> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.dest == dest)
            .ok_or_else(|| DotlingError::NotTracked(PathBuf::from(dest)))?;
        Ok(self.entries.remove(idx))
    }

    /// Finds an entry by its destination path.
    pub fn find_by_dest(&self, dest: &str) -> Option<&LinkEntry> {
        self.entries.iter().find(|e| e.dest == dest)
    }

    /// Finds an entry by its source path.
    #[allow(dead_code)]
    pub fn find_by_src(&self, src: &str) -> Option<&LinkEntry> {
        self.entries.iter().find(|e| e.src == src)
    }

    /// Returns entries that match the current platform.
    ///
    /// Entries with [`Platform::All`] are always included.
    pub fn active_entries(&self) -> Vec<&LinkEntry> {
        self.entries.iter().filter(|e| e.os.is_active()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_serialize_deserialize() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            path: dir.path().join(CONFIG_FILE),
            encryption: EncryptionConfig {
                recipients: vec!["age1test".to_string()],
            },
            entries: vec![
                LinkEntry {
                    src: "config/nvim/init.lua".to_string(),
                    dest: "~/.config/nvim/init.lua".to_string(),
                    method: LinkMethod::Symlink,
                    os: Platform::All,
                },
                LinkEntry {
                    src: "shell/zshrc".to_string(),
                    dest: "~/.zshrc".to_string(),
                    method: LinkMethod::Copy,
                    os: Platform::Macos,
                },
            ],
        };
        config.save().unwrap();

        let loaded = Config::load(dir.path()).unwrap();
        assert_eq!(loaded.encryption.recipients.len(), 1);
        assert_eq!(loaded.encryption.recipients[0], "age1test");
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].src, "config/nvim/init.lua");
        assert_eq!(loaded.entries[0].dest, "~/.config/nvim/init.lua");
        assert_eq!(loaded.entries[0].method, LinkMethod::Symlink);
        assert_eq!(loaded.entries[1].src, "shell/zshrc");
        assert_eq!(loaded.entries[1].method, LinkMethod::Copy);
        assert_eq!(loaded.entries[1].os, Platform::Macos);
    }

    #[test]
    fn duplicate_dest_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config {
            path: dir.path().join(CONFIG_FILE),
            encryption: EncryptionConfig::default(),
            entries: Vec::new(),
        };
        config
            .add_entry(LinkEntry {
                src: "shell/zshrc".to_string(),
                dest: "~/.zshrc".to_string(),
                method: LinkMethod::Symlink,
                os: Platform::All,
            })
            .unwrap();

        let result = config.add_entry(LinkEntry {
            src: "other/zshrc".to_string(),
            dest: "~/.zshrc".to_string(),
            method: LinkMethod::Symlink,
            os: Platform::All,
        });
        assert!(result.is_err());
    }

    #[test]
    fn remove_nonexistent_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config {
            path: dir.path().join(CONFIG_FILE),
            encryption: EncryptionConfig::default(),
            entries: Vec::new(),
        };
        let result = config.remove_entry("~/.nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn load_empty_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path()).unwrap();
        assert!(config.entries.is_empty());
    }
}
